use pdf_writer::types::{ActionType, AnnotationType, Predictor};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str};

use crate::font::{font_name, write_embedded, EmbeddedRefs, Font};
use crate::image::{ColorSpace, Encoding, Image};
use crate::paint::{image_name, PageLink};
use crate::{FontId, Page, HELVETICA_BOLD};

pub(crate) fn finish_pdf(
    page: Page,
    contents: Vec<Content>,
    fonts: Vec<Font>,
    used_fonts: Vec<bool>,
    images: Vec<Image>,
    used_images: Vec<bool>,
    links: Vec<Vec<PageLink>>,
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
    let first_page = next;
    let count = contents.len();
    let page_refs = (0..count).map(|index| Ref::new(first_page + index as i32 * 2));
    let mut next_annotation = first_page + count as i32 * 2;

    pdf.catalog(catalog).pages(pages);
    pdf.pages(pages).kids(page_refs.clone()).count(count as i32);

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

    for (index, (content, page_links)) in contents.into_iter().zip(links).enumerate() {
        let page_ref = Ref::new(first_page + index as i32 * 2);
        let stream_ref = Ref::new(first_page + 1 + index as i32 * 2);
        let mut output_page = pdf.page(page_ref);
        output_page
            .parent(pages)
            .media_box(Rect::new(0.0, 0.0, page.width.0, page.height.0))
            .contents(stream_ref);
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
                .border(0.0, 0.0, 0.0, None);
            annotation
                .action()
                .action_type(ActionType::Uri)
                .uri(Str(link.uri.as_bytes()));
        }
    }

    pdf.finish()
}

/// Fixed level keeps the output byte-for-byte deterministic across runs.
pub(crate) fn deflate(bytes: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(bytes, 6)
}

fn write_image(pdf: &mut Pdf, reference: Ref, mask: Option<Ref>, image: &Image) {
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
    }
    if let Some(key) = &image.color_key {
        xobject.color_mask(key.iter().copied());
    }
    if let Some(mask) = mask {
        xobject.s_mask(mask);
    }
}
