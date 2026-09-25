use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};

use crate::font::{font_name, write_embedded, EmbeddedRefs, Font};
use crate::{FontId, Page, HELVETICA_BOLD};

pub(crate) fn finish_pdf(
    page: Page,
    contents: Vec<Content>,
    fonts: Vec<Font>,
    used_fonts: Vec<bool>,
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
        .try_fold(8 * 1024_usize, |total, length| total.checked_add(length));
    let mut pdf = capacity
        .map(|capacity| Pdf::with_capacity(capacity.max(8 * 1024)))
        .unwrap_or_else(Pdf::new);
    let catalog = Ref::new(1);
    let pages = Ref::new(2);

    // Refs run: catalog, pages, one dictionary per used font, then a page and
    // a stream per page.
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
    let first_page = next;
    let count = contents.len();
    let page_refs = (0..count).map(|index| Ref::new(first_page + index as i32 * 2));

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

    for (index, content) in contents.into_iter().enumerate() {
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
        resources.finish();
        output_page.finish();
        pdf.stream(stream_ref, &deflate(&content.finish()))
            .filter(Filter::FlateDecode);
    }

    pdf.finish()
}

/// Fixed level keeps the output byte-for-byte deterministic across runs.
pub(crate) fn deflate(bytes: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(bytes, 6)
}
