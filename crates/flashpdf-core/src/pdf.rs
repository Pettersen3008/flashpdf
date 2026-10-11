use pdf_writer::types::{
    ActionType, AnnotationFlags, AnnotationType, AssociationKind, OutputIntentSubtype, Predictor,
    StructRole,
};
use pdf_writer::writers::{ExtGraphicsState, Resources, StructTreeRoot};
use pdf_writer::{Content, Filter, Finish, Name, Null, Pdf, Rect, Ref, Str, TextStr};

use crate::font::{font_name, write_embedded, EmbeddedRefs, Font};
use crate::image::{ColorSpace, Encoding, Image};
use crate::paint::{image_name, PageLink, PageTag};
use crate::vector::alpha_name;
use crate::{FontId, Page, HELVETICA_BOLD};

#[derive(Default)]
pub(crate) struct Metadata {
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) subject: String,
    pub(crate) keywords: String,
    pub(crate) language: String,
    pub(crate) tagged: bool,
    pub(crate) pdfa: bool,
    /// Factur-X conformance level and embedded file name.
    pub(crate) facturx: Option<(&'static str, &'static str)>,
    /// Sorted by name, as the EmbeddedFiles name tree requires.
    pub(crate) attachments: Vec<Attachment>,
    /// In document order. Each becomes a destination object, and a bookmark when it has a level.
    pub(crate) anchors: Vec<Anchor>,
    /// Destination name and anchor slot, sorted by name as the Dests name tree requires.
    pub(crate) names: Vec<(String, u16)>,
}

impl Metadata {
    /// Where `name` sits in `names`, or where it would go.
    pub(crate) fn find(&self, name: &str) -> Result<usize, usize> {
        (self.names).binary_search_by(|(other, _)| other.as_str().cmp(name))
    }
}

pub(crate) struct Anchor {
    /// The heading level of a bookmark, or 0.
    pub(crate) level: u8,
    pub(crate) title: String,
    /// Page index, then the left and top of the destination; set once a line paints.
    pub(crate) at: Option<(usize, f32, f32)>,
}

pub(crate) struct Attachment {
    pub(crate) name: String,
    pub(crate) mime: String,
    pub(crate) description: String,
    pub(crate) relationship: AssociationKind,
    pub(crate) data: Vec<u8>,
}

// sRGB-v2-micro.icc from github.com/saucecontrol/Compact-ICC-Profiles, CC0 1.0.
const SRGB: &[u8] = include_bytes!("sRGB-v2-micro.icc");

#[allow(clippy::too_many_arguments)]
pub(crate) fn finish_pdf(
    page: Page,
    contents: Vec<Content>,
    fonts: Vec<Font>,
    used_fonts: Vec<bool>,
    images: Vec<Image>,
    used_images: Vec<bool>,
    links: Vec<Vec<PageLink>>,
    tags: Vec<Vec<PageTag>>,
    alphas: Vec<Vec<[u8; 2]>>,
    metadata: Metadata,
) -> Vec<u8> {
    let used: Vec<FontId> = (0..fonts.len())
        .filter(|slot| used_fonts[*slot])
        .map(|slot| FontId::new(slot as u8))
        .collect();
    let capacity = contents
        .iter()
        .map(Content::len)
        .chain(used.iter().filter_map(|slot| match &fonts[slot.index()] {
            Font::Embedded(font) => Some(font.bytes.len()),
            Font::Helvetica { .. } => None,
        }))
        .chain(images.iter().map(|image| image.data.len()))
        .try_fold(8 * 1024_usize, |total, length| total.checked_add(length));
    let mut pdf = capacity
        .map(|capacity| Pdf::with_capacity(capacity.max(8 * 1024)))
        .unwrap_or_else(Pdf::new);
    let catalog = Ref::new(1);
    let pages = Ref::new(2);

    // Refs run: catalog, pages, one dictionary per used font, one or two
    // streams per used image, then a page and a stream per page, then the
    // link annotations.
    let mut next = 3;
    let mut allocate = || {
        let reference = Ref::new(next);
        next += 1;
        reference
    };
    struct FontRefs {
        slot: FontId,
        font: Ref,
        descriptor: Option<Ref>,
        file: Option<Ref>,
        descendant: Option<Ref>,
        to_unicode: Option<Ref>,
    }
    let font_refs: Vec<_> = used
        .iter()
        .map(|slot| {
            let font = allocate();
            let embedded = matches!(fonts[slot.index()], Font::Embedded(_));
            FontRefs {
                slot: *slot,
                font,
                descriptor: embedded.then(&mut allocate),
                file: embedded.then(&mut allocate),
                descendant: embedded.then(&mut allocate),
                to_unicode: embedded.then(&mut allocate),
            }
        })
        .collect();
    let image_refs: Vec<(u16, Ref, Option<Ref>)> = (0..images.len())
        .filter(|slot| used_images[*slot])
        .map(|slot| {
            let image = allocate();
            let mask = images[slot].alpha.is_some().then(&mut allocate);
            (slot as u16, image, mask)
        })
        .collect();
    let info = [
        &metadata.title,
        &metadata.author,
        &metadata.subject,
        &metadata.keywords,
    ]
    .iter()
    .any(|value| !value.is_empty())
    .then(&mut allocate);
    let structure = metadata.tagged.then(|| (allocate(), allocate()));
    let parent_arrays: Vec<Ref> = if metadata.tagged {
        tags.iter().map(|_| allocate()).collect()
    } else {
        Vec::new()
    };
    let tag_refs: Vec<Vec<Ref>> = if metadata.tagged {
        tags.iter()
            .map(|page| page.iter().map(|_| allocate()).collect())
            .collect()
    } else {
        Vec::new()
    };
    let archive = metadata.pdfa.then(|| (allocate(), allocate()));
    let files: Vec<(Ref, Ref)> = metadata
        .attachments
        .iter()
        .map(|_| (allocate(), allocate()))
        .collect();
    let dests: Vec<Ref> = metadata.anchors.iter().map(|_| allocate()).collect();
    let bookmarks: Vec<Bookmark> = metadata
        .anchors
        .iter()
        .zip(&dests)
        .filter(|(anchor, _)| anchor.level > 0)
        .map(|(anchor, dest)| (allocate(), *dest, anchor))
        .collect();
    let outline = (!bookmarks.is_empty()).then(&mut allocate);
    let first_page = next;
    let count = contents.len();
    let page_refs = (0..count).map(|index| Ref::new(first_page + index as i32 * 2));
    let mut next_annotation = first_page + count as i32 * 2;

    let mut catalog_writer = pdf.catalog(catalog);
    catalog_writer.pages(pages);
    if !metadata.language.is_empty() {
        catalog_writer.lang(pdf_writer::TextStr(&metadata.language));
    }
    if let Some(root) = outline {
        catalog_writer
            .outlines(root)
            .pair(Name(b"PageMode"), Name(b"UseOutlines"));
    }
    if let Some((root, _)) = structure {
        catalog_writer.pair(Name(b"StructTreeRoot"), root);
        catalog_writer.mark_info().marked(true);
    }
    if let Some((xmp, profile)) = archive {
        catalog_writer.metadata(xmp);
        catalog_writer
            .output_intents()
            .push()
            .subtype(OutputIntentSubtype::PDFA)
            .output_condition_identifier(TextStr("sRGB IEC61966-2.1"))
            .dest_output_profile(profile);
    }
    if !files.is_empty() {
        catalog_writer
            .insert(Name(b"AF"))
            .array()
            .items(files.iter().map(|(spec, _)| *spec));
    }
    if !files.is_empty() || !metadata.names.is_empty() {
        let mut names = catalog_writer.names();
        if !metadata.names.is_empty() {
            let mut tree = names.destinations();
            let mut entries = tree.names();
            for (name, slot) in &metadata.names {
                entries.insert(Str(name.as_bytes()), dests[usize::from(*slot)]);
            }
        }
        if !files.is_empty() {
            let mut tree = names.embedded_files();
            let mut entries = tree.names();
            for (attachment, (spec, _)) in metadata.attachments.iter().zip(&files) {
                entries.insert(Str(attachment.name.as_bytes()), *spec);
            }
        }
    }
    drop(catalog_writer);
    if let Some(info) = info {
        let mut writer = pdf.document_info(info);
        if !metadata.title.is_empty() {
            writer.title(pdf_writer::TextStr(&metadata.title));
        }
        if !metadata.author.is_empty() {
            writer.author(pdf_writer::TextStr(&metadata.author));
        }
        if !metadata.subject.is_empty() {
            writer.subject(pdf_writer::TextStr(&metadata.subject));
        }
        if !metadata.keywords.is_empty() {
            writer.keywords(pdf_writer::TextStr(&metadata.keywords));
        }
    }
    pdf.pages(pages).kids(page_refs.clone()).count(count as i32);

    // `[page /XYZ left top null]`, as an object so the name tree and bookmarks share it.
    for (anchor, reference) in metadata.anchors.iter().zip(&dests) {
        let (page_index, left, top) = anchor.at.unwrap();
        pdf.indirect(*reference)
            .array()
            .item(Ref::new(first_page + page_index as i32 * 2))
            .item(Name(b"XYZ"))
            .item(left)
            .item(top)
            .item(Null);
    }
    if let Some(root) = outline {
        let last = write_outline(&mut pdf, root, &bookmarks);
        let mut writer = pdf.outline(root);
        writer
            .first(bookmarks[0].0)
            .last(last)
            .count(bookmarks.len() as i32);
    }

    if let Some((root, document)) = structure {
        let mut tree = pdf.indirect(root).start::<StructTreeRoot>();
        tree.child(document).parent_tree_next_key(count as i32);
        let mut parents = tree.parent_tree();
        let mut nums = parents.nums();
        for (index, reference) in parent_arrays.iter().enumerate() {
            nums.insert(index as i32, *reference);
        }
        drop(nums);
        drop(parents);
        drop(tree);
        let mut doc = pdf.struct_element(document);
        doc.kind(StructRole::Document).parent(root);
        doc.children().items(tag_refs.iter().flatten().copied());
        drop(doc);
        for (page_index, page_tags) in tags.iter().enumerate() {
            pdf.indirect(parent_arrays[page_index])
                .array()
                .items(tag_refs[page_index].iter().copied());
            for (mcid, tag) in page_tags.iter().enumerate() {
                let mut item = pdf.struct_element(tag_refs[page_index][mcid]);
                item.kind(match tag {
                    PageTag::Paragraph => StructRole::P,
                    PageTag::Figure(_) => StructRole::Figure,
                })
                .parent(document)
                .page(Ref::new(first_page + page_index as i32 * 2));
                if let PageTag::Figure(alt) = tag {
                    item.alt(TextStr(alt));
                }
                item.marked_content_child().marked_content_id(mcid as i32);
            }
        }
    }

    for refs in &font_refs {
        match &fonts[refs.slot.index()] {
            Font::Helvetica { .. } => {
                pdf.type1_font(refs.font)
                    .base_font(Name(if refs.slot == HELVETICA_BOLD {
                        b"Helvetica-Bold"
                    } else {
                        b"Helvetica"
                    }))
                    .encoding_predefined(Name(b"WinAnsiEncoding"));
            }
            Font::Embedded(font) => write_embedded(
                &mut pdf,
                EmbeddedRefs {
                    font: refs.font,
                    descendant: refs.descendant.unwrap(),
                    descriptor: refs.descriptor.unwrap(),
                    file: refs.file.unwrap(),
                    to_unicode: refs.to_unicode.unwrap(),
                },
                font,
            ),
        }
    }
    drop(fonts);

    for (slot, reference, mask) in &image_refs {
        write_image(&mut pdf, *reference, *mask, &images[usize::from(*slot)]);
    }
    drop(images);

    if let Some((xmp, profile)) = archive {
        pdf.metadata(xmp, xmp_packet(&metadata).as_bytes());
        pdf.icc_profile(profile, SRGB).n(3);
    }
    for (attachment, (spec, file)) in metadata.attachments.iter().zip(&files) {
        let data = deflate(&attachment.data);
        let mut stream = pdf.embedded_file(*file, &data);
        stream.subtype(Name(attachment.mime.as_bytes()));
        stream.filter(Filter::FlateDecode);
        drop(stream);
        let mut file_spec = pdf.file_spec(*spec);
        file_spec
            .path(Str(attachment.name.as_bytes()))
            .unic_file(TextStr(&attachment.name))
            .association_kind(attachment.relationship)
            .embedded_file(*file);
        if !attachment.description.is_empty() {
            file_spec.description(TextStr(&attachment.description));
        }
    }

    for (index, ((content, page_links), page_alphas)) in
        contents.into_iter().zip(links).zip(alphas).enumerate()
    {
        let page_ref = Ref::new(first_page + index as i32 * 2);
        let stream_ref = Ref::new(first_page + 1 + index as i32 * 2);
        let mut output_page = pdf.page(page_ref);
        output_page
            .parent(pages)
            .media_box(Rect::new(0.0, 0.0, page.width.0, page.height.0))
            .contents(stream_ref);
        if metadata.tagged {
            output_page.struct_parents(index as i32);
        }
        let mut resources = output_page.resources();
        // Every font belongs to one `/Font` dictionary; a second `fonts()` call
        // would write a duplicate key and hide the earlier entries.
        let mut resource_fonts = resources.fonts();
        for refs in &font_refs {
            let mut buffer = [0_u8; 4];
            resource_fonts.pair(Name(font_name(refs.slot, &mut buffer)), refs.font);
        }
        resource_fonts.finish();
        if !image_refs.is_empty() {
            let mut x_objects = resources.x_objects();
            for (slot, reference, _) in &image_refs {
                let mut buffer = [0_u8; 8];
                x_objects.pair(Name(image_name(*slot, &mut buffer)), *reference);
            }
            x_objects.finish();
        }
        write_alphas(&mut resources, &page_alphas);
        resources.finish();
        let annotations: Vec<Ref> = page_links
            .iter()
            .map(|_| {
                let reference = Ref::new(next_annotation);
                next_annotation += 1;
                reference
            })
            .collect();
        if !annotations.is_empty() {
            output_page.annotations(annotations.iter().copied());
        }
        output_page.finish();
        pdf.stream(stream_ref, &deflate(&content.finish()))
            .filter(Filter::FlateDecode);
        for (link, reference) in page_links.iter().zip(annotations) {
            let [x1, y1, x2, y2] = link.rect;
            let mut annotation = pdf.annotation(reference);
            annotation
                .subtype(AnnotationType::Link)
                .rect(Rect::new(x1, y1, x2, y2))
                .flags(AnnotationFlags::PRINT)
                .border(0.0, 0.0, 0.0, None);
            let mut action = annotation.action();
            match link.uri.strip_prefix('#') {
                Some(name) => action
                    .action_type(ActionType::GoTo)
                    .pair(Name(b"D"), Str(name.as_bytes())),
                None => action
                    .action_type(ActionType::Uri)
                    .uri(Str(link.uri.as_bytes())),
            };
        }
    }

    if metadata.pdfa {
        // FNV-1a over every object written: deterministic, and distinct per document.
        let id = pdf
            .as_bytes()
            .iter()
            .fold(0x6c62272e07bb014262b821756295c58d_u128, |hash, byte| {
                (hash ^ u128::from(*byte)).wrapping_mul(0x0000000001000000000000000000013b)
            })
            .to_be_bytes()
            .to_vec();
        pdf.set_file_id((id.clone(), id));
    }
    pdf.finish()
}

/// An outline item: its object, its destination object, and its anchor.
type Bookmark<'a> = (Ref, Ref, &'a Anchor);

/// Writes `items` as siblings under `parent` and returns the last one. An item's children are
/// the items after it with a higher level, so a heading nests under the nearest earlier
/// heading of a lower level.
fn write_outline(pdf: &mut Pdf, parent: Ref, items: &[Bookmark]) -> Ref {
    let level = items[0].2.level;
    let starts: Vec<usize> = (0..items.len())
        .filter(|&index| items[index].2.level <= level)
        .collect();
    let ends = starts.iter().skip(1).copied().chain([items.len()]);
    let mut prev = None;
    for (&start, end) in starts.iter().zip(ends) {
        let (object, dest, anchor) = items[start];
        let kids = &items[start + 1..end];
        let last = (!kids.is_empty()).then(|| write_outline(pdf, object, kids));
        let mut item = pdf.outline_item(object);
        item.title(TextStr(&anchor.title))
            .parent(parent)
            .pair(Name(b"Dest"), dest);
        if let Some(prev) = prev {
            item.prev(prev);
        }
        if let Some((next, ..)) = items.get(end) {
            item.next(*next);
        }
        if let (Some((first, ..)), Some(last)) = (kids.first(), last) {
            item.first(*first).last(last).count(kids.len() as i32);
        }
        prev = Some(object);
    }
    prev.unwrap()
}

/// XMP must agree with the Info dictionary, so it repeats the same fields plus the language.
fn xmp_packet(metadata: &Metadata) -> String {
    let mut xmp = String::from(concat!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>",
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">",
        "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">",
        "<rdf:Description rdf:about=\"\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"",
        " xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\">",
        "<pdfaid:part>3</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance>",
    ));
    for (value, open, close) in [
        (
            &metadata.title,
            "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">",
            "</rdf:li></rdf:Alt></dc:title>",
        ),
        (
            &metadata.author,
            "<dc:creator><rdf:Seq><rdf:li>",
            "</rdf:li></rdf:Seq></dc:creator>",
        ),
        (
            &metadata.subject,
            "<dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">",
            "</rdf:li></rdf:Alt></dc:description>",
        ),
        (
            &metadata.language,
            "<dc:language><rdf:Bag><rdf:li>",
            "</rdf:li></rdf:Bag></dc:language>",
        ),
        (&metadata.keywords, "<pdf:Keywords>", "</pdf:Keywords>"),
    ] {
        if value.is_empty() {
            continue;
        }
        xmp.push_str(open);
        for character in value.chars() {
            match character {
                '&' => xmp.push_str("&amp;"),
                '<' => xmp.push_str("&lt;"),
                '>' => xmp.push_str("&gt;"),
                _ => xmp.push(character),
            }
        }
        xmp.push_str(close);
    }
    xmp.push_str("</rdf:Description>");
    if let Some((level, file)) = metadata.facturx {
        xmp.extend([
            concat!(
                "<rdf:Description rdf:about=\"\"",
                " xmlns:fx=\"urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#\">",
                "<fx:DocumentType>INVOICE</fx:DocumentType><fx:DocumentFileName>",
            ),
            file,
            "</fx:DocumentFileName><fx:Version>1.0</fx:Version><fx:ConformanceLevel>",
            level,
            concat!(
                "</fx:ConformanceLevel></rdf:Description>",
                "<rdf:Description rdf:about=\"\"",
                " xmlns:pdfaExtension=\"http://www.aiim.org/pdfa/ns/extension/\"",
                " xmlns:pdfaSchema=\"http://www.aiim.org/pdfa/ns/schema#\"",
                " xmlns:pdfaProperty=\"http://www.aiim.org/pdfa/ns/property#\">",
                "<pdfaExtension:schemas><rdf:Bag><rdf:li rdf:parseType=\"Resource\">",
                "<pdfaSchema:schema>Factur-X PDFA Extension Schema</pdfaSchema:schema>",
                "<pdfaSchema:namespaceURI>urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#",
                "</pdfaSchema:namespaceURI><pdfaSchema:prefix>fx</pdfaSchema:prefix>",
                "<pdfaSchema:property><rdf:Seq>",
            ),
        ]);
        for (name, description) in [
            ("DocumentFileName", "The name of the embedded XML document"),
            (
                "DocumentType",
                "The type of the hybrid document, e.g. INVOICE",
            ),
            (
                "Version",
                "The version of the standard applying to the embedded XML",
            ),
            (
                "ConformanceLevel",
                "The conformance level of the embedded XML",
            ),
        ] {
            xmp.extend([
                "<rdf:li rdf:parseType=\"Resource\"><pdfaProperty:name>",
                name,
                concat!(
                    "</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType>",
                    "<pdfaProperty:category>external</pdfaProperty:category>",
                    "<pdfaProperty:description>",
                ),
                description,
                "</pdfaProperty:description></rdf:li>",
            ]);
        }
        xmp.push_str(concat!(
            "</rdf:Seq></pdfaSchema:property></rdf:li></rdf:Bag>",
            "</pdfaExtension:schemas></rdf:Description>",
        ));
    }
    xmp.push_str("</rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>");
    xmp
}

/// Fixed level keeps the output byte-for-byte deterministic across runs.
pub(crate) fn deflate(bytes: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(bytes, 6)
}

/// One `/ExtGState` per fill and stroke alpha pair, named by `alpha_name`.
#[inline(never)]
fn write_alphas(resources: &mut Resources, alphas: &[[u8; 2]]) {
    if alphas.is_empty() {
        return;
    }
    let mut states = resources.ext_g_states();
    for alpha in alphas {
        states
            .insert(Name(&alpha_name(*alpha)))
            .start::<ExtGraphicsState>()
            .non_stroking_alpha(f32::from(alpha[0]) / 255.0)
            .stroking_alpha(f32::from(alpha[1]) / 255.0);
    }
}

fn write_image(pdf: &mut Pdf, reference: Ref, mask: Option<Ref>, image: &Image) {
    if let Encoding::Form { alphas, .. } = &image.encoding {
        // A form drawn in the unit square paints exactly like an image.
        let mut form = pdf.form_xobject(reference, &image.data);
        form.filter(Filter::FlateDecode);
        form.bbox(Rect::new(0.0, 0.0, 1.0, 1.0));
        write_alphas(&mut form.resources(), alphas);
        return;
    }
    let (width, height) = (image.width as i32, image.height as i32);
    if let (Some(mask), Some(alpha)) = (mask, &image.alpha) {
        let mut smask = pdf.image_xobject(mask, alpha);
        smask
            .width(width)
            .height(height)
            .bits_per_component(8)
            .filter(Filter::FlateDecode);
        smask.color_space().device_gray();
    }
    let mut xobject = pdf.image_xobject(reference, &image.data);
    xobject.width(width).height(height).bits_per_component(8);
    match &image.color_space {
        ColorSpace::Gray => xobject.color_space().device_gray(),
        ColorSpace::Rgb => xobject.color_space().device_rgb(),
        ColorSpace::Cmyk => xobject.color_space().device_cmyk(),
        ColorSpace::Indexed(palette) => xobject.color_space().indexed(
            Name(b"DeviceRGB"),
            (palette.len() / 3 - 1) as i32,
            palette,
        ),
    }
    match image.encoding {
        Encoding::Dct { inverted } => {
            xobject.filter(Filter::DctDecode);
            if inverted {
                xobject.decode([1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]);
            }
        }
        Encoding::FlatePredicted => {
            xobject.filter(Filter::FlateDecode);
            xobject
                .decode_parms()
                .predictor(Predictor::PngOptimum)
                .colors(image.color_space.channels() as i32)
                .bits_per_component(8)
                .columns(width);
        }
        Encoding::Flate => {
            xobject.filter(Filter::FlateDecode);
        }
        Encoding::Form { .. } => unreachable!("written as a form above"),
    }
    if let Some(key) = &image.color_key {
        xobject.color_mask(key.iter().copied());
    }
    if let Some(mask) = mask {
        xobject.s_mask(mask);
    }
}
