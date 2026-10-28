use pdf_writer::types::{
    ActionType, AnnotationFlags, AnnotationType, AssociationKind, OutputIntentSubtype, Predictor,
    StructRole,
};
use pdf_writer::writers::{ExtGraphicsState, Resources, StructTreeRoot};
use pdf_writer::{Chunk, Filter, Finish, Name, Null, Pdf, Rect, Ref, Settings, Str, TextStr};

use crate::font::{base_font, font_name, write_embedded, EmbeddedRefs, Font};
use crate::image::{ColorSpace, Encoding, Image};
use crate::paint::{image_name, PageLink, PageTag};
use crate::vector::alpha_name;
use crate::{FontId, Page};

#[derive(Default)]
pub(crate) struct Metadata {
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) subject: String,
    pub(crate) keywords: String,
    pub(crate) language: String,
    pub(crate) tagged: bool,
    pub(crate) pdfua: bool,
    pub(crate) structure: Vec<crate::compile::StructureNode>,
    pub(crate) pdfa: Option<PdfA>,
    /// Factur-X conformance level and embedded file name.
    pub(crate) facturx: Option<(&'static str, &'static str)>,
    /// Sorted by name, as the EmbeddedFiles name tree requires.
    pub(crate) attachments: Vec<Attachment>,
    /// In document order. Each becomes a destination object, and a bookmark when it has a level.
    pub(crate) anchors: Vec<Anchor>,
    /// Destination name and anchor slot, sorted by name as the Dests name tree requires.
    pub(crate) names: Vec<(String, u16)>,
}

/// A PDF/A part and conformance level.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PdfA {
    A2b,
    /// Level B plus a Unicode mapping for every glyph, which `/ToUnicode` already gives.
    A2u,
    A3b,
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
    contents: Vec<Vec<u8>>,
    fonts: Vec<Font>,
    used_fonts: Vec<bool>,
    images: Vec<Image>,
    used_images: Vec<bool>,
    mut links: Vec<Vec<PageLink>>,
    tags: Vec<Vec<PageTag>>,
    alphas: Vec<Vec<[u8; 2]>>,
    shadings: Vec<Vec<crate::paint::Shading>>,
    mut metadata: Metadata,
) -> Vec<u8> {
    if metadata.pdfua {
        for link in links.iter_mut().flatten() {
            if link_node(&metadata.structure, link.structure).is_none() {
                let id = metadata.structure.len() as u32 + 1;
                metadata.structure.push(crate::compile::StructureNode {
                    at: None,
                    role: "Link",
                    parent: 1,
                    children: Vec::new(),
                    alt: link.uri.clone(),
                    lang: None,
                    scope: None,
                    columns: 1,
                    rows: 1,
                });
                metadata.structure[0].children.push(id);
                link.structure = id;
            }
        }
    }
    let used: Vec<FontId> = (0..fonts.len())
        .filter(|slot| used_fonts[*slot])
        .map(|slot| FontId::new(slot as u8))
        .collect();
    // Subsetting and deflating are per font and per page and independent, so they may run in
    // parallel. Doing them first sizes the output buffer from the deflated lengths.
    let embedded: Vec<&crate::EmbeddedFont> = used
        .iter()
        .filter_map(|slot| match &fonts[slot.index()] {
            Font::Embedded(font) => Some(&**font),
            Font::Standard(_) => None,
        })
        .collect();
    let programs = map_ordered(
        embedded,
        |font| font.bytes.len(),
        crate::EmbeddedFont::deflated_program,
    );
    let attached = map_ordered(
        metadata.attachments.iter().collect(),
        |attachment| attachment.data.len(),
        |attachment| deflate(&attachment.data, Stream::Binary),
    );
    let count = contents.len();
    let streams = map_ordered(contents, Vec::len, |content| {
        deflate(&content, Stream::Content)
    });
    let capacity = (streams.iter().chain(&attached))
        .chain(programs.iter().map(|program| &program.1))
        .map(Vec::len)
        .chain(images.iter().map(|image| image.data.len()))
        .try_fold(8 * 1024 + count * 512, |total, length| {
            total.checked_add(length)
        });
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
    let shading_refs: Vec<Vec<Ref>> = shadings
        .iter()
        .map(|page| page.iter().map(|_| allocate()).collect())
        .collect();
    for (page, refs) in shadings.iter().zip(&shading_refs) {
        crate::paint::write_shadings(&mut pdf, page, refs);
    }
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
    let object_stream = metadata.tagged.then(&mut allocate);
    let parent_arrays: Vec<Ref> = if metadata.tagged {
        tags.iter().map(|_| allocate()).collect()
    } else {
        Vec::new()
    };
    let tag_refs: Vec<Vec<Ref>> = if metadata.tagged && !metadata.pdfua {
        tags.iter()
            .map(|page| page.iter().map(|_| allocate()).collect())
            .collect()
    } else {
        Vec::new()
    };
    let ua_refs: Vec<Ref> = if metadata.pdfua {
        metadata
            .structure
            .iter()
            .enumerate()
            .map(|(index, _)| {
                if index == 0 {
                    structure.unwrap().1
                } else {
                    allocate()
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let archive = metadata.pdfa.map(|_| (allocate(), allocate()));
    let xmp = archive
        .map(|(xmp, _)| xmp)
        .or_else(|| metadata.pdfua.then(&mut allocate));
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
    if metadata.pdfua {
        catalog_writer.viewer_preferences().display_doc_title(true);
    }
    if let Some(xmp) = xmp {
        catalog_writer.metadata(xmp);
    }
    if let Some((_, profile)) = archive {
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

    // The structure tree is hundreds of small dictionaries, so it goes compact into one
    // object stream. `packed[i]` spans `starts[i]..starts[i + 1]` of `chunk`.
    let mut packed = Vec::new();
    if let (Some((root, document)), Some(stream)) = (structure, object_stream) {
        let mut chunk = Chunk::with_settings(Settings { pretty: false });
        let mut starts = vec![0];
        if metadata.pdfua {
            write_ua_structure(
                &mut chunk,
                &metadata,
                &tags,
                &links,
                &ua_refs,
                &parent_arrays,
                root,
                first_page,
                &mut packed,
                &mut starts,
            );
        } else {
            let mut tree = chunk.indirect(root).start::<StructTreeRoot>();
            tree.child(document).parent_tree_next_key(count as i32);
            let mut parents = tree.parent_tree();
            let mut nums = parents.nums();
            for (index, reference) in parent_arrays.iter().enumerate() {
                nums.insert(index as i32, *reference);
            }
            drop(nums);
            drop(parents);
            drop(tree);
            starts.push(chunk.len());
            let mut doc = chunk.struct_element(document);
            doc.kind(StructRole::Document).parent(root);
            doc.children().items(tag_refs.iter().flatten().copied());
            drop(doc);
            starts.push(chunk.len());
            packed.extend([root, document]);
            for (page_index, page_tags) in tags.iter().enumerate() {
                chunk
                    .indirect(parent_arrays[page_index])
                    .array()
                    .items(tag_refs[page_index].iter().copied());
                starts.push(chunk.len());
                packed.push(parent_arrays[page_index]);
                for (mcid, tag) in page_tags.iter().enumerate() {
                    let mut item = chunk.struct_element(tag_refs[page_index][mcid]);
                    item.kind(match tag {
                        PageTag::Paragraph | PageTag::Semantic(_) => StructRole::P,
                        PageTag::Figure(_) => StructRole::Figure,
                    })
                    .parent(document)
                    .page(Ref::new(first_page + page_index as i32 * 2));
                    if let PageTag::Figure(alt) = tag {
                        item.alt(TextStr(alt));
                    }
                    // An integer child is a marked-content sequence on the element's /Pg.
                    item.pair(Name(b"K"), mcid as i32);
                    drop(item);
                    starts.push(chunk.len());
                    packed.push(tag_refs[page_index][mcid]);
                }
            }
        }
        let (data, first) = pack_objects(chunk.as_bytes(), &packed, &starts);
        pdf.stream(stream, &deflate(&data, Stream::Table))
            .filter(Filter::FlateDecode)
            .pair(Name(b"Type"), Name(b"ObjStm"))
            .pair(Name(b"N"), packed.len() as i32)
            .pair(Name(b"First"), first as i32);
    }

    let mut programs = programs.into_iter();
    for refs in &font_refs {
        match &fonts[refs.slot.index()] {
            Font::Standard(face) => {
                pdf.type1_font(refs.font)
                    .base_font(Name(base_font(*face)))
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
                &programs.next().unwrap(),
            ),
        }
    }
    drop(fonts);

    for (slot, reference, mask) in &image_refs {
        write_image(&mut pdf, *reference, *mask, &images[usize::from(*slot)]);
    }
    drop(images);

    if let Some(xmp) = xmp {
        pdf.metadata(xmp, xmp_packet(&metadata).as_bytes());
    }
    if let Some((_, profile)) = archive {
        pdf.icc_profile(profile, SRGB).n(3);
    }
    for ((attachment, (spec, file)), data) in metadata.attachments.iter().zip(&files).zip(attached)
    {
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

    let mut annotation_key = count as i32;
    for (index, ((stream, page_links), page_alphas)) in
        streams.into_iter().zip(links).zip(alphas).enumerate()
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
        if !shading_refs[index].is_empty() {
            let mut patterns = resources.patterns();
            for (i, reference) in shading_refs[index].iter().enumerate() {
                patterns.pair(Name(format!("Gr{i}").as_bytes()), *reference);
            }
        }
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
            if metadata.pdfua {
                output_page.pair(Name(b"Tabs"), Name(b"S"));
            }
            output_page.annotations(annotations.iter().copied());
        }
        output_page.finish();
        pdf.stream(stream_ref, &stream).filter(Filter::FlateDecode);
        for (link, reference) in page_links.iter().zip(annotations) {
            let [x1, y1, x2, y2] = link.rect;
            let mut annotation = pdf.annotation(reference);
            annotation
                .subtype(AnnotationType::Link)
                .rect(Rect::new(x1, y1, x2, y2))
                .flags(AnnotationFlags::PRINT)
                .border(0.0, 0.0, 0.0, None);
            if metadata.pdfua {
                let node = link_node(&metadata.structure, link.structure);
                let alt = node
                    .map(|id| metadata.structure[id as usize - 1].alt.as_str())
                    .filter(|alt| !alt.trim().is_empty())
                    .unwrap_or(&link.uri);
                annotation
                    .pair(Name(b"StructParent"), annotation_key)
                    .pair(Name(b"P"), page_ref)
                    .pair(Name(b"Contents"), TextStr(alt));
                annotation_key += 1;
            }
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

    // FNV-1a over every object written: deterministic, and distinct per document.
    let id = metadata.pdfa.map(|_| {
        pdf.as_bytes()
            .iter()
            .fold(0x6c62272e07bb014262b821756295c58d_u128, |hash, byte| {
                (hash ^ u128::from(*byte)).wrapping_mul(0x0000000001000000000000000000013b)
            })
            .to_be_bytes()
    });
    if let Some(stream) = object_stream {
        return finish_with_xref_stream(pdf, stream, &packed, catalog, info, id);
    }
    if let Some(id) = id {
        pdf.set_file_id((id.to_vec(), id.to_vec()));
    }
    pdf.finish()
}

fn link_node(nodes: &[crate::compile::StructureNode], mut id: u32) -> Option<u32> {
    while id != 0 {
        let node = nodes.get(id as usize - 1)?;
        if node.role == "Link" {
            return Some(id);
        }
        id = node.parent;
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn write_ua_structure(
    chunk: &mut Chunk,
    metadata: &Metadata,
    tags: &[Vec<PageTag>],
    links: &[Vec<PageLink>],
    refs: &[Ref],
    arrays: &[Ref],
    root: Ref,
    first_page: i32,
    packed: &mut Vec<Ref>,
    starts: &mut Vec<usize>,
) {
    let nodes = &metadata.structure;
    let mut marked = vec![Vec::new(); nodes.len()];
    let mut objects = vec![Vec::new(); nodes.len()];
    let mut live = vec![false; nodes.len()];
    let mut include = |mut id: u32| {
        while id != 0 && !live[id as usize - 1] {
            live[id as usize - 1] = true;
            id = nodes[id as usize - 1].parent;
        }
    };
    include(1);
    // Empty cells keep their column positions; empty list bodies still belong to their items.
    for (index, node) in nodes.iter().enumerate() {
        if matches!(
            node.role,
            "TH" | "TD" | "LBody" | "H1" | "H2" | "H3" | "H4" | "H5" | "H6"
        ) {
            include(index as u32 + 1);
        }
    }
    for (page, tags) in tags.iter().enumerate() {
        for (mcid, tag) in tags.iter().enumerate() {
            if let PageTag::Semantic(id) = tag {
                include(*id);
                marked[*id as usize - 1].push((page, mcid));
            }
        }
    }
    let mut annotation = first_page + tags.len() as i32 * 2;
    let mut key = tags.len() as i32;
    let mut parents = Vec::new();
    for (page, links) in links.iter().enumerate() {
        for link in links {
            let id = link_node(nodes, link.structure).unwrap();
            include(id);
            objects[id as usize - 1].push((page, Ref::new(annotation)));
            parents.push((key, refs[id as usize - 1]));
            key += 1;
            annotation += 1;
        }
    }
    let mut tree = chunk.indirect(root).start::<StructTreeRoot>();
    tree.child(refs[0]).parent_tree_next_key(key);
    let mut parent_tree = tree.parent_tree();
    let mut nums = parent_tree.nums();
    for (page, reference) in arrays.iter().enumerate() {
        nums.insert(page as i32, *reference);
    }
    for (key, reference) in parents {
        nums.insert(key, reference);
    }
    drop(nums);
    drop(parent_tree);
    drop(tree);
    packed.push(root);
    starts.push(chunk.len());
    for (page, tags) in tags.iter().enumerate() {
        let mut array = chunk.indirect(arrays[page]).array();
        for tag in tags {
            if let PageTag::Semantic(id) = tag {
                array.item(refs[*id as usize - 1]);
            }
        }
        drop(array);
        packed.push(arrays[page]);
        starts.push(chunk.len());
    }
    for (index, node) in nodes.iter().enumerate().filter(|(index, _)| live[*index]) {
        let mut item = chunk.struct_element(refs[index]);
        item.pair(Name(b"S"), Name(node.role.as_bytes()));
        item.parent(if node.parent == 0 {
            root
        } else {
            refs[node.parent as usize - 1]
        });
        if !node.alt.is_empty() {
            item.alt(TextStr(&node.alt));
        }
        if let Some(lang) = &node.lang {
            item.pair(Name(b"Lang"), TextStr(lang));
        }
        if matches!(node.role, "TH" | "TD") {
            let mut attributes = item.insert(Name(b"A")).dict();
            attributes.pair(Name(b"O"), Name(b"Table"));
            if let Some(scope) = &node.scope {
                attributes.pair(
                    Name(b"Scope"),
                    Name(match scope.as_str() {
                        "row" => b"Row",
                        "both" => b"Both",
                        _ => b"Column",
                    }),
                );
            }
            if node.columns > 1 {
                attributes.pair(Name(b"ColSpan"), i32::from(node.columns));
            }
            if node.rows > 1 {
                attributes.pair(Name(b"RowSpan"), i32::from(node.rows));
            }
        }
        let mut kids = item.insert(Name(b"K")).array();
        for child in &node.children {
            if live[*child as usize - 1] {
                kids.item(refs[*child as usize - 1]);
            }
        }
        for &(page, mcid) in &marked[index] {
            kids.push()
                .dict()
                .pair(Name(b"Type"), Name(b"MCR"))
                .pair(Name(b"Pg"), Ref::new(first_page + page as i32 * 2))
                .pair(Name(b"MCID"), mcid as i32);
        }
        for &(page, reference) in &objects[index] {
            kids.push()
                .dict()
                .pair(Name(b"Type"), Name(b"OBJR"))
                .pair(Name(b"Pg"), Ref::new(first_page + page as i32 * 2))
                .pair(Name(b"Obj"), reference);
        }
        drop(kids);
        drop(item);
        packed.push(refs[index]);
        starts.push(chunk.len());
    }
}

/// Object stream data for `packed`, written compact into `chunk` with `packed[i]` spanning
/// `starts[i]..starts[i + 1]`. Returns the data and its `/First` offset.
fn pack_objects(chunk: &[u8], packed: &[Ref], starts: &[usize]) -> (Vec<u8>, usize) {
    let mut header = Vec::new();
    let mut body = Vec::with_capacity(chunk.len());
    for (reference, span) in packed.iter().zip(starts.windows(2)) {
        let mut id = itoa::Buffer::new();
        let id = id.format(reference.get());
        // Strip the `<id> 0 obj\n` and `\nendobj\n` that compact settings write.
        let object = &chunk[span[0] + id.len() + b" 0 obj\n".len()..span[1] - b"\nendobj\n".len()];
        header.extend_from_slice(id.as_bytes());
        header.push(b' ');
        header.extend_from_slice(itoa::Buffer::new().format(body.len()).as_bytes());
        header.push(b' ');
        body.extend_from_slice(object);
        body.push(b'\n');
    }
    let first = header.len();
    header.extend(body);
    (header, first)
}

/// Finishes `pdf` with a cross-reference stream (PDF 1.5) in place of the classic table, so
/// the `packed` objects can live in object stream `stream`, at their index in `packed`.
fn finish_with_xref_stream(
    pdf: Pdf,
    stream: Ref,
    packed: &[Ref],
    root: Ref,
    info: Option<Ref>,
    id: Option<[u8; 16]>,
) -> Vec<u8> {
    let mut bytes = pdf.finish();
    // pdf-writer keeps object offsets private, so read them back from the table it ends
    // with: `xref\n0 <size>\n`, one 20-byte entry per object, the trailer, then
    // `startxref\n<offset>\n%%EOF`.
    let number = |digits: &[u8]| {
        digits
            .iter()
            .fold(0, |value, digit| value * 10 + usize::from(digit - b'0'))
    };
    let end = bytes.len() - b"\n%%EOF".len();
    let mark = bytes[..end]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .unwrap();
    let table = number(&bytes[mark + 1..end]);
    let start = table + b"xref\n0 ".len();
    let line = bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .unwrap();
    let size = number(&bytes[start..start + line]);
    let entries = start + line + 1;
    let mut offsets: Vec<Option<usize>> = (0..size)
        .map(|index| {
            let entry = &bytes[entries + index * 20..][..20];
            (entry[17] == b'n').then(|| number(&entry[..10]))
        })
        .collect();
    bytes.truncate(table);
    // The xref stream itself is the last object, where the table was.
    offsets.push(Some(table));
    let mut slots = vec![None; offsets.len()];
    for (index, reference) in packed.iter().enumerate() {
        slots[reference.get() as usize] = Some(index);
    }
    let width = |value: usize| (usize::BITS - value.leading_zeros()).div_ceil(8) as usize;
    let middle = width(table.max(stream.get() as usize));
    let last = width(packed.len().max(0xffff));
    let mut data = Vec::with_capacity(offsets.len() * (1 + middle + last));
    for (number, (offset, slot)) in offsets.iter().zip(&slots).enumerate() {
        let (kind, field, index) = match (offset, slot) {
            (Some(offset), _) => (1, *offset, 0),
            (None, Some(index)) => (2, stream.get() as usize, *index),
            (None, None) => (0, 0, if number == 0 { 0xffff } else { 0 }),
        };
        data.push(kind);
        data.extend_from_slice(&field.to_be_bytes()[size_of::<usize>() - middle..]);
        data.extend_from_slice(&index.to_be_bytes()[size_of::<usize>() - last..]);
    }
    let data = deflate(&data, Stream::Table);
    let mut chunk = Chunk::new();
    let mut xref = chunk.stream(Ref::new(size as i32), &data);
    xref.filter(Filter::FlateDecode)
        .pair(Name(b"Type"), Name(b"XRef"))
        .pair(Name(b"Size"), offsets.len() as i32)
        .pair(Name(b"Root"), root);
    if let Some(info) = info {
        xref.pair(Name(b"Info"), info);
    }
    if let Some(id) = &id {
        xref.insert(Name(b"ID")).array().items([Str(id), Str(id)]);
    }
    xref.insert(Name(b"W"))
        .array()
        .items([1, middle as i32, last as i32]);
    drop(xref);
    bytes.extend_from_slice(chunk.as_bytes());
    bytes.extend_from_slice(b"startxref\n");
    bytes.extend_from_slice(itoa::Buffer::new().format(table).as_bytes());
    bytes.extend_from_slice(b"\n%%EOF");
    bytes
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
    ));
    xmp.push_str(match metadata.pdfa {
        Some(PdfA::A2b) => "<pdfaid:part>2</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance>",
        Some(PdfA::A2u) => "<pdfaid:part>2</pdfaid:part><pdfaid:conformance>U</pdfaid:conformance>",
        Some(PdfA::A3b) => "<pdfaid:part>3</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance>",
        None => "",
    });
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
    if metadata.pdfua {
        xmp.push_str(concat!(
            "<rdf:Description rdf:about=\"\" xmlns:pdfuaid=\"http://www.aiim.org/pdfua/ns/id/\">",
            "<pdfuaid:part>1</pdfuaid:part></rdf:Description>",
        ));
        if metadata.pdfa.is_some() {
            xmp.push_str(concat!(
                "<rdf:Description rdf:about=\"\" xmlns:pdfaExtension=\"http://www.aiim.org/pdfa/ns/extension/\"",
                " xmlns:pdfaSchema=\"http://www.aiim.org/pdfa/ns/schema#\" xmlns:pdfaProperty=\"http://www.aiim.org/pdfa/ns/property#\">",
                "<pdfaExtension:schemas><rdf:Bag><rdf:li rdf:parseType=\"Resource\">",
                "<pdfaSchema:schema>PDF/UA identification schema</pdfaSchema:schema>",
                "<pdfaSchema:namespaceURI>http://www.aiim.org/pdfua/ns/id/</pdfaSchema:namespaceURI>",
                "<pdfaSchema:prefix>pdfuaid</pdfaSchema:prefix><pdfaSchema:property><rdf:Seq>",
                "<rdf:li rdf:parseType=\"Resource\"><pdfaProperty:name>part</pdfaProperty:name>",
                "<pdfaProperty:valueType>Integer</pdfaProperty:valueType><pdfaProperty:category>internal</pdfaProperty:category>",
                "<pdfaProperty:description>PDF/UA part</pdfaProperty:description></rdf:li>",
                "</rdf:Seq></pdfaSchema:property></rdf:li></rdf:Bag></pdfaExtension:schemas></rdf:Description>",
            ));
        }
    }
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

/// Below 64 KiB of input, rayon's handoff costs more than the work it spreads. Tests always
/// spread, so the parallel path runs on small documents too.
#[cfg(feature = "parallel")]
fn worth_spreading(size: usize) -> bool {
    cfg!(test) || size >= 64 * 1024
}

/// Maps `items` in order. With the `parallel` feature and enough input, rayon spreads the
/// calls across threads; `collect` keeps the order, so the bytes match the serial path.
pub(crate) fn map_ordered<T: Send, R: Send>(
    items: Vec<T>,
    size: impl Fn(&T) -> usize,
    work: impl Fn(T) -> R + Sync + Send,
) -> Vec<R> {
    #[cfg(feature = "parallel")]
    if items.len() > 1 && worth_spreading(items.iter().map(&size).sum()) {
        use rayon::prelude::*;
        return items.into_par_iter().map(work).collect();
    }
    #[cfg(not(feature = "parallel"))]
    let _ = size;
    items.into_iter().map(work).collect()
}

/// Runs two independent jobs, on two threads under the same rule as `map_ordered`.
pub(crate) fn join<A: Send, B: Send>(
    size: usize,
    a: impl FnOnce() -> A + Send,
    b: impl FnOnce() -> B + Send,
) -> (A, B) {
    #[cfg(feature = "parallel")]
    if worth_spreading(size) {
        return rayon::join(a, b);
    }
    #[cfg(not(feature = "parallel"))]
    let _ = size;
    (a(), b())
}

/// What a stream holds, which picks how hard `deflate` works on it.
#[derive(Clone, Copy)]
pub(crate) enum Stream {
    /// Page and form operators.
    Content,
    /// Font programs, CMaps, object and cross-reference streams.
    Table,
    /// Image samples and attachments.
    Binary,
}

/// Fixed settings keep the output byte-for-byte deterministic across runs. On the 80-line
/// invoice, content at level 5 filtered and tables at level 4 deflate about a third faster
/// than level 6 for 0.1% more bytes; binary data was not measured, so it keeps level 6.
pub(crate) fn deflate(bytes: &[u8], stream: Stream) -> Vec<u8> {
    use miniz_oxide::deflate::core::{
        compress_to_output, create_comp_flags_from_zip_params, CompressionStrategy,
        CompressorOxide, TDEFLFlush, TDEFLStatus,
    };
    let (level, strategy) = match stream {
        // Filtered drops matches under 5 bytes, which in operators are mostly digit runs.
        Stream::Content => (5, CompressionStrategy::Filtered),
        Stream::Table => (4, CompressionStrategy::Default),
        Stream::Binary => (6, CompressionStrategy::Default),
    };
    // Window bits above 0 select the zlib wrapper FlateDecode expects.
    let flags = create_comp_flags_from_zip_params(level, 1, strategy as i32);
    let mut compressor = CompressorOxide::new(flags);
    let mut output = Vec::with_capacity(bytes.len() / 4 + 64);
    let (status, _) = compress_to_output(&mut compressor, bytes, TDEFLFlush::Finish, |chunk| {
        output.extend_from_slice(chunk);
        true
    });
    assert!(matches!(status, TDEFLStatus::Done), "deflate stopped early");
    output
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
