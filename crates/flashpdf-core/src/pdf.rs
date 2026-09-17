use pdf_writer::{types::FontFlags, Content, Finish, Name, Pdf, Rect, Ref};

use crate::font::{font_name, Font};
use crate::{FontId, Page, HELVETICA_BOLD};

/// Appends a page and returns its body cursor.
pub(crate) fn start_page(page: Page, contents: &mut Vec<Content>) -> f32 {
    contents.push(Content::new());
    page.height.0 - page.margin.0
}

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
            Font::Embedded(font) => {
                let name = format!("FlashPDF{}", refs.slot.slot());
                let mut output = pdf.indirect(refs.font).dict();
                output.pair(Name(b"Type"), Name(b"Font"));
                output.pair(Name(b"Subtype"), Name(b"TrueType"));
                output.pair(Name(b"BaseFont"), Name(name.as_bytes()));
                output.pair(Name(b"FirstChar"), 32);
                output.pair(Name(b"LastChar"), 255);
                output.insert(Name(b"Widths")).array().items(
                    font.widths
                        .iter()
                        .map(|width| i32::from(width.unwrap_or(0))),
                );
                output.pair(Name(b"FontDescriptor"), refs.descriptor.unwrap());
                output.pair(Name(b"Encoding"), Name(b"WinAnsiEncoding"));
                output.finish();

                pdf.font_descriptor(refs.descriptor.unwrap())
                    .name(Name(name.as_bytes()))
                    .flags(FontFlags::NON_SYMBOLIC)
                    .bbox(font.bbox)
                    .italic_angle(0.0)
                    .ascent(font.ascent)
                    .descent(font.descent)
                    .cap_height(font.cap_height)
                    .stem_v(80.0)
                    .font_file2(refs.file.unwrap());
                pdf.stream(refs.file.unwrap(), &font.bytes).finish();
            }
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
        pdf.stream(stream_ref, &content.finish());
    }

    pdf.finish()
}
