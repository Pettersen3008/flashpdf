use super::{
    Decoder, HEADER_LEN, MAGIC, MAX_BLOCK_BYTES, MAX_FONT_BYTES, MAX_IMAGE_BYTES, VERSION,
};
use crate::renderer::MAX_PAGES;
use crate::OwnedCommand;

fn header(version: u16, width: f32, height: f32, margin: f32) -> Vec<u8> {
    page_header(version, width, height, [margin; 4])
}

/// Margins in CSS order: top, right, bottom, left.
fn page_header(version: u16, width: f32, height: f32, margin: [f32; 4]) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.extend(version.to_le_bytes());
    assert_eq!(bytes.len(), HEADER_LEN);
    let mut page = width.to_le_bytes().to_vec();
    page.extend(height.to_le_bytes());
    for side in margin {
        page.extend(side.to_le_bytes());
    }
    bytes.extend(record(1, &page));
    bytes
}

fn record(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![opcode];
    bytes.extend((payload.len() as u32).to_le_bytes());
    bytes.extend(payload);
    bytes
}

fn text(value: &[u8]) -> Vec<u8> {
    let mut bytes = (value.len() as u32).to_le_bytes().to_vec();
    bytes.extend(value);
    bytes
}

fn columns(values: &[(u8, f32)]) -> Vec<u8> {
    let plain: Vec<_> = values
        .iter()
        .map(|(kind, value)| (*kind, *value, &[0][..]))
        .collect();
    slotted(&plain)
}

/// Columns with their slot bytes: the flags, then a u16 row span when bit 2 is set.
fn slotted(values: &[(u8, f32, &[u8])]) -> Vec<u8> {
    let mut bytes = (values.len() as u16).to_le_bytes().to_vec();
    for (kind, value, slot) in values {
        bytes.push(*kind);
        bytes.extend(value.to_le_bytes());
        bytes.extend(*slot);
    }
    bytes
}

/// `(font, size, colour, flags, text)`.
type Run<'a> = (u8, f32, [u8; 3], u16, &'a [u8]);

fn paragraph(align: u8, runs: &[Run<'_>]) -> Vec<u8> {
    let runs: Vec<LinkedRun<'_>> = runs
        .iter()
        .map(|(font, size, color, flags, value)| (*font, *size, *color, *flags, None, *value))
        .collect();
    linked_paragraph(align, &[], &runs)
}

/// `(font, size, colour, flags, link index, text)`; a link index sets flag bit 3.
type LinkedRun<'a> = (u8, f32, [u8; 3], u16, Option<u16>, &'a [u8]);

/// `links` is the URI table the runs' link indices point into.
fn linked_paragraph(align: u8, links: &[&[u8]], runs: &[LinkedRun<'_>]) -> Vec<u8> {
    // One orphan and one widow, so lines break wherever the page ends.
    let mut payload = vec![align, 1, 0, 1, 0];
    payload.extend((links.len() as u16).to_le_bytes());
    for link in links {
        payload.extend(text(link));
    }
    payload.extend((runs.len() as u16).to_le_bytes());
    for (font, size, color, flags, link, value) in runs {
        payload.push(*font);
        payload.extend(size.to_le_bytes());
        payload.extend(color);
        payload.extend((*flags | if link.is_some() { 8 } else { 0 }).to_le_bytes());
        if let Some(link) = link {
            payload.extend(link.to_le_bytes());
        }
        payload.extend(text(value));
    }
    record(9, &payload)
}

/// `header_rows`, no footer rows, then a full-width fraction track.
fn table_start(header_rows: u16) -> Vec<u8> {
    let mut bytes = header_rows.to_le_bytes().to_vec();
    bytes.extend(0_u16.to_le_bytes());
    bytes.push(1);
    bytes.extend(1.0_f32.to_le_bytes());
    bytes
}

fn plain(value: &[u8]) -> Vec<u8> {
    paragraph(0, &[(0, 10.0, [0, 0, 0], 0, value)])
}

/// An Anchor record: a destination `name` (may be empty), and a bookmark when `level` is 1 to 6.
fn anchor(name: &str, level: u8, title: &str) -> Vec<u8> {
    let mut payload = text(name.as_bytes());
    payload.push(level);
    payload.extend(text(title.as_bytes()));
    record(22, &payload)
}

fn box_start() -> Vec<u8> {
    decorated_box(0.0, &[255, 0, 0])
}

/// A box with `border` on every side, then the opacity, border styles, and radius bytes.
fn decorated_box(border: f32, tail: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    for edge in [0.0; 8].into_iter().chain([border; 4]) {
        payload.extend(edge.to_le_bytes());
    }
    payload.extend([1, 0xee, 0xee, 0xee]);
    payload.extend([0, 0, 0].repeat(4));
    payload.extend(tail);
    payload.extend([0; 7]);
    record(17, &payload)
}

/// Every opcode the JSX lowering emits, on a 120x60 page whose 40pt of body
/// forces the stream across five pages.
fn representative() -> Vec<u8> {
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    bytes.extend(record(13, &[1]));
    bytes.extend(record(3, &0.0_f32.to_le_bytes()));
    bytes.extend(paragraph(0, &[(0, 1.0, [0, 0, 0], 0, b"\x1f")]));
    bytes.extend(record(4, &[]));
    bytes.extend(anchor("top", 1, "Invoice"));
    bytes.extend(plain(b"Before"));
    bytes.extend(anchor("", 0, ""));
    bytes.extend(box_start());
    bytes.extend(plain(b"Boxed"));
    bytes.extend(record(18, &[]));
    bytes.extend(record(5, &columns(&[(0, 50.0), (1, 1.0)])));
    bytes.extend(plain(b"row"));
    bytes.extend(anchor("cell", 2, "Cell"));
    bytes.extend(plain(b"1"));
    bytes.extend(record(6, &[]));
    bytes.extend(record(10, &table_start(1)));
    for cell in [b"head", b"body"] {
        bytes.extend(record(5, &columns(&[(1, 1.0)])));
        bytes.extend(plain(cell));
        bytes.extend(record(6, &[]));
    }
    bytes.extend(record(11, &[]));
    // A middle-aligned cell two columns wide and two rows tall beside a bottom-aligned one.
    bytes.extend(record(3, &0.0_f32.to_le_bytes()));
    let span: &[u8] = &[5, 2, 0];
    bytes.extend(record(
        5,
        &slotted(&[(1, 1.0, span), (1, 1.0, &[8]), (1, 1.0, &[2])]),
    ));
    bytes.extend(plain(b"span"));
    bytes.extend(plain(b"x"));
    bytes.extend(record(6, &[]));
    bytes.extend(record(
        5,
        &slotted(&[(1, 1.0, &[16]), (1, 1.0, &[16]), (1, 1.0, &[0])]),
    ));
    bytes.extend(plain(b"y"));
    bytes.extend(record(6, &[]));
    bytes.extend(record(4, &[]));
    bytes.extend(record(2, &5.0_f32.to_le_bytes()));
    bytes.extend(record(3, &0.0_f32.to_le_bytes()));
    bytes.extend(plain(b"Stacked"));
    bytes.extend(record(4, &[]));
    bytes.extend(linked_paragraph(
        1,
        &[b"https://example.com", b"#top"],
        &[
            (1, 10.0, [0xcc, 0, 0], 2, Some(0), b"Styled"),
            (1, 10.0, [0xcc, 0, 0], 2, Some(1), b" top"),
        ],
    ));
    bytes.extend(image_record(0, Some(20.0), None, Some(b"mailto:a@b.c")));
    bytes.extend(record(7, &[]));
    bytes.extend(plain(b"After"));
    bytes.extend(record(255, &[]));
    bytes
}

/// A decoder with one registered 1x1 grey PNG for `representative()`'s image record.
fn decoder() -> Decoder {
    let mut decoder = Decoder::default();
    decoder.add_image(png(1, 1, 0, 8, &[0, 128])).unwrap();
    decoder
}

fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = decoder();
    decoder.push(bytes).map_err(|error| error.to_string())?;
    decoder.finish().map_err(|error| error.to_string())
}

#[test]
fn given_a_footer_and_a_first_page_footer_when_decoding_then_page_one_paints_the_first_and_later_pages_resolve_numbers(
) {
    let mut bytes = header(VERSION, 595.0, 842.0, 36.0);
    bytes.extend(record(8, &[0]));
    bytes.extend(plain("Page \u{1e} of \u{1f}".as_bytes()));
    bytes.extend(record(8, &[1]));
    bytes.extend(plain(b"Cover"));
    bytes.extend(plain(b"First"));
    bytes.extend(record(7, &[]));
    bytes.extend(plain(b"Second"));
    bytes.extend(record(255, &[]));

    let pdf = lopdf::Document::load_mem(&decode(&bytes).unwrap()).unwrap();
    assert_eq!(pdf.extract_text(&[1]).unwrap().trim(), "First\nCover");
    assert_eq!(
        pdf.extract_text(&[2]).unwrap().trim(),
        "Second\nPage 2 of 2"
    );
}

#[test]
fn given_a_paragraph_with_bold_and_break_runs_when_decoding_then_paints_one_line_in_two_fonts() {
    let mut bytes = header(VERSION, 595.0, 842.0, 36.0);
    bytes.extend(paragraph(
        0,
        &[
            (0, 10.0, [0, 0, 0], 0, b"Hello "),
            (1, 10.0, [0xcc, 0, 0], 1, b"World"),
            (0, 10.0, [0, 0, 0], 0, b"again"),
        ],
    ));
    bytes.extend(record(255, &[]));
    let pdf = lopdf::Document::load_mem(&decode(&bytes).unwrap()).unwrap();
    assert_eq!(crate::tests::page_text(&pdf, 1), "Hello World\nagain");
    let page = *pdf.get_pages().values().next().unwrap();
    let content = String::from_utf8(pdf.get_page_content(page)).unwrap();
    let objects: Vec<&str> = content.split("ET").collect();
    assert_eq!(objects.len(), 3, "{content}");
    assert!(objects[0].contains("/F1 10 Tf") && objects[0].contains("/F2 10 Tf"));
    assert!(objects[0].contains("0.8 0 0 rg"), "{content}");
    assert!(objects[1].contains("(again) Tj"), "{content}");
}

fn push_error(bytes: &[u8]) -> String {
    let mut decoder = Decoder::default();
    decoder.push(bytes).unwrap_err().to_string()
}

#[test]
fn given_embedded_regular_and_bold_fonts_when_decoding_then_pdf_embeds_both() {
    let font_bytes = include_bytes!("../../../../packages/flashpdf/test/fixtures/Abel-Regular.ttf");
    let mut decoder = Decoder::default();
    assert_eq!(decoder.add_font(font_bytes.to_vec()).unwrap(), 12);
    assert_eq!(decoder.add_font(font_bytes.to_vec()).unwrap(), 13);
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    for slot in [12, 13] {
        bytes.extend(paragraph(0, &[(slot, 10.0, [0, 0, 0], 0, b"Font")]));
    }
    bytes.extend(record(255, &[]));
    decoder.push(&bytes).unwrap();
    let pdf = decoder.finish().unwrap();
    let document = lopdf::Document::load_mem(&pdf).unwrap();
    let page = *document.get_pages().values().next().unwrap();
    let fonts = document.get_page_fonts(page).unwrap();
    assert_eq!(fonts.len(), 2);
    for font in fonts.values() {
        assert_eq!(font.get(b"Subtype").unwrap().as_name().unwrap(), b"Type0");
        assert!(font
            .get(b"BaseFont")
            .unwrap()
            .as_name()
            .unwrap()
            .ends_with(b"+Abel-Regular"));
        let descendant = font.get(b"DescendantFonts").unwrap().as_array().unwrap()[0]
            .as_reference()
            .unwrap();
        let descriptor = document
            .get_dictionary(descendant)
            .unwrap()
            .get(b"FontDescriptor")
            .unwrap()
            .as_reference()
            .unwrap();
        let descriptor = document.get_dictionary(descriptor).unwrap();
        assert_eq!(
            descriptor.get(b"CapHeight").unwrap().as_float().unwrap(),
            700.1953
        );
        let file = descriptor
            .get(b"FontFile2")
            .unwrap()
            .as_reference()
            .unwrap();
        let file = document.get_object(file).unwrap().as_stream().unwrap();
        let program = miniz_oxide::inflate::decompress_to_vec_zlib(&file.content).unwrap();
        assert_eq!(
            file.dict.get(b"Length1").unwrap().as_i64().unwrap(),
            program.len() as i64
        );
        assert!(program.len() < font_bytes.len(), "{}", program.len());
    }
}

#[test]
fn given_every_split_point_when_decoding_then_output_is_valid_and_deterministic() {
    let bytes = representative();
    let expected = decode(&bytes).unwrap();
    assert!(expected.starts_with(b"%PDF-"));
    let pdf = lopdf::Document::load_mem(&expected).unwrap();
    assert_eq!(pdf.get_pages().len(), 5);
    for (index, expected) in [
        "Before\nBoxed\nrow\n1\n5",
        "head\nbody",
        "span\nx\ny\nStacked",
        "Styled top",
        "After",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(crate::tests::page_text(&pdf, index as u32 + 1), *expected);
    }
    for split in 0..=bytes.len() {
        let mut decoder = decoder();
        decoder.push(&bytes[..split]).unwrap();
        decoder.push(&bytes[split..]).unwrap();
        assert_eq!(decoder.finish().unwrap(), expected, "split {split}");
    }
}

#[test]
fn given_bad_headers_records_and_text_when_decoding_then_rejects() {
    let mut truncated = representative();
    truncated.pop();
    assert_eq!(decode(&truncated).unwrap_err(), "truncated protocol");

    let mut bad_magic = representative();
    bad_magic[0] = b'X';
    assert_eq!(push_error(&bad_magic), "invalid magic");

    for version in [1_u16, 20] {
        let mut bad_version = representative();
        bad_version[4..6].copy_from_slice(&version.to_le_bytes());
        assert_eq!(push_error(&bad_version), "unknown version");
    }

    let mut unknown = header(VERSION, 120.0, 60.0, 10.0);
    unknown.extend(record(42, &[]));
    assert_eq!(push_error(&unknown), "unknown opcode");

    for legacy in [16, 19] {
        let mut removed = header(VERSION, 120.0, 60.0, 10.0);
        removed.extend(record(legacy, &[]));
        assert_eq!(push_error(&removed), "unknown opcode");
    }

    let mut oversized_record = header(VERSION, 120.0, 60.0, 10.0);
    oversized_record.push(1);
    oversized_record.extend(65_537_u32.to_le_bytes());
    assert_eq!(push_error(&oversized_record), "record too large");

    let mut invalid_utf8 = header(VERSION, 120.0, 60.0, 10.0);
    invalid_utf8.extend(plain(&[0xff]));
    assert_eq!(push_error(&invalid_utf8), "invalid UTF-8");

    for control in ["\u{0b}", "\u{1e}", "\u{1f}", "\u{7f}", "\u{85}"] {
        let mut sentinel = header(VERSION, 120.0, 60.0, 10.0);
        sentinel.extend(plain(control.as_bytes()));
        assert_eq!(push_error(&sentinel), "unsupported control character");
    }

    // Printable text passes the boundary; Helvetica then rejects what WinAnsi lacks.
    let mut unsupported = header(VERSION, 120.0, 60.0, 10.0);
    unsupported.extend(plain("🙂".as_bytes()));
    assert_eq!(push_error(&unsupported), "UnsupportedCharacter('🙂')");

    let mut bad_length = header(VERSION, 120.0, 60.0, 10.0);
    let mut payload = vec![0, 1, 0, 1, 0, 0, 0, 1, 0, 0];
    payload.extend(10.0_f32.to_le_bytes());
    payload.extend([0, 0, 0, 0, 0]);
    payload.extend(4_u32.to_le_bytes());
    payload.push(b'x');
    bad_length.extend(record(9, &payload));
    assert_eq!(push_error(&bad_length), "truncated record");

    let mut no_widows = header(VERSION, 120.0, 60.0, 10.0);
    let mut zero = plain(b"x");
    zero[8] = 0;
    no_widows.extend(zero);
    assert_eq!(push_error(&no_widows), "invalid orphans or widows");

    let mut bad_flags = header(VERSION, 120.0, 60.0, 10.0);
    bad_flags.extend(paragraph(0, &[(0, 10.0, [0, 0, 0], 3 << 13, b"x")]));
    assert_eq!(push_error(&bad_flags), "invalid run flags");

    let mut late_marker = header(VERSION, 120.0, 60.0, 10.0);
    late_marker.extend(paragraph(
        0,
        &[
            (0, 10.0, [0, 0, 0], 0, b"x"),
            (0, 10.0, [0, 0, 0], 1 << 13, b"1."),
        ],
    ));
    assert_eq!(push_error(&late_marker), "invalid marker run");

    let mut bad_align = header(VERSION, 120.0, 60.0, 10.0);
    bad_align.extend(paragraph(6, &[(0, 10.0, [0, 0, 0], 0, b"x")]));
    assert_eq!(push_error(&bad_align), "invalid text alignment");

    let mut unknown_font = header(VERSION, 120.0, 60.0, 10.0);
    unknown_font.extend(paragraph(0, &[(12, 10.0, [0, 0, 0], 0, b"x")]));
    assert_eq!(push_error(&unknown_font), "unknown font");

    let mut invalid_metadata = header(VERSION, 120.0, 60.0, 10.0);
    let mut fields = vec![1];
    for _ in 0..5 {
        fields.extend(text(b""));
    }
    invalid_metadata.extend(record(14, &fields));
    assert_eq!(push_error(&invalid_metadata), "invalid metadata language");

    let mut duplicate_header = header(VERSION, 120.0, 60.0, 10.0);
    for _ in 0..2 {
        duplicate_header.extend(record(13, &[0]));
        duplicate_header.extend(plain(b"Header"));
    }
    assert_eq!(push_error(&duplicate_header), "InvalidLayout");

    let mut bad_page = header(VERSION, 120.0, 60.0, 10.0);
    bad_page.extend(record(8, &[2]));
    assert_eq!(push_error(&bad_page), "invalid furniture page");

    for after in [record(7, &[]), record(255, &[]), record(13, &[1])] {
        let mut empty = header(VERSION, 120.0, 60.0, 10.0);
        empty.extend(record(8, &[0]));
        empty.extend(after);
        assert_eq!(push_error(&empty), "invalid nesting");
    }

    let mut late = header(VERSION, 120.0, 60.0, 10.0);
    late.extend(plain(b"Body"));
    late.extend(record(13, &[0]));
    assert_eq!(push_error(&late), "invalid nesting");
}

#[test]
fn given_invalid_box_decoration_when_decoding_then_rejects() {
    let radius = |first: f32| {
        let mut tail = vec![255, 0, 1, 0];
        for value in [first, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0] {
            tail.extend(value.to_le_bytes());
        }
        tail
    };
    let mut unequal = radius(2.0);
    unequal[1] = 1; // the top border is dashed and the others solid
    for (border, tail, message) in [
        (1.0, vec![255, 3, 0], "invalid border style"),
        (1.0, vec![255, 0, 2], "invalid border radius flag"),
        (1.0, radius(f32::NAN), "invalid border radius"),
        (1.0, radius(-1.0), "invalid border radius"),
        (1.0, unequal, "invalid border radius with unequal borders"),
    ] {
        let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
        bytes.extend(decorated_box(border, &tail));
        assert_eq!(push_error(&bytes), message);
    }
    let mut rounded = header(VERSION, 120.0, 60.0, 10.0);
    rounded.extend(decorated_box(1.0, &radius(2.0)));
    rounded.extend(plain(b"x"));
    rounded.extend(record(18, &[]));
    rounded.extend(record(255, &[]));
    assert!(decode(&rounded).is_ok());
}

#[test]
fn given_spacing_beyond_a_page_side_or_an_invalid_furniture_flag_when_decoding_then_rejects() {
    for (flag, value, error) in [
        (64_u16, f32::NAN, "invalid letter spacing"),
        (128, -14_401.0, "invalid word spacing"),
    ] {
        let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
        let mut payload = vec![0, 1, 0, 1, 0, 0, 0, 1, 0, 0];
        payload.extend(10.0_f32.to_le_bytes());
        payload.extend([0, 0, 0]);
        payload.extend(flag.to_le_bytes());
        payload.extend(value.to_le_bytes());
        payload.extend(text(b"x"));
        bytes.extend(record(9, &payload));
        assert_eq!(push_error(&bytes), error);
    }
    let mut invalid_header = header(VERSION, 120.0, 60.0, 10.0);
    invalid_header.extend(record(13, &[3]));
    assert_eq!(push_error(&invalid_header), "invalid furniture page");
}

#[test]
fn given_invalid_numbers_layout_and_rows_when_decoding_then_rejects() {
    assert_eq!(
        push_error(&header(VERSION, f32::NAN, 60.0, 10.0)),
        "invalid page width"
    );
    assert_eq!(
        push_error(&header(VERSION, 120.0, 60.0, 30.0)),
        "InvalidPage"
    );

    let mut zero_size = header(VERSION, 120.0, 60.0, 10.0);
    zero_size.extend(paragraph(0, &[(0, 0.0, [0, 0, 0], 0, b"x")]));
    assert_eq!(push_error(&zero_size), "invalid font size");

    let mut infinite_column = header(VERSION, 120.0, 60.0, 10.0);
    infinite_column.extend(record(5, &columns(&[(0, f32::INFINITY)])));
    assert_eq!(push_error(&infinite_column), "invalid column width");

    let mut zero_columns = header(VERSION, 120.0, 60.0, 10.0);
    zero_columns.extend(record(5, &0_u16.to_le_bytes()));
    assert_eq!(push_error(&zero_columns), "invalid column count");

    let mut too_many_columns = header(VERSION, 120.0, 60.0, 10.0);
    too_many_columns.extend(record(5, &257_u16.to_le_bytes()));
    assert_eq!(push_error(&too_many_columns), "invalid column count");

    let mut bad_close = header(VERSION, 120.0, 60.0, 10.0);
    bad_close.extend(record(4, &[]));
    assert_eq!(push_error(&bad_close), "invalid nesting");

    let mut deep = header(VERSION, 120.0, 60.0, 10.0);
    for _ in 0..65 {
        deep.extend(record(3, &0.0_f32.to_le_bytes()));
    }
    assert_eq!(push_error(&deep), "nesting exceeds 64");

    let mut bad_table_close = header(VERSION, 120.0, 60.0, 10.0);
    bad_table_close.extend(record(11, &[]));
    assert_eq!(push_error(&bad_table_close), "invalid nesting");

    let mut headers_only = header(VERSION, 120.0, 60.0, 10.0);
    headers_only.extend(record(10, &table_start(1)));
    headers_only.extend(record(11, &[]));
    assert_eq!(push_error(&headers_only), "InvalidLayout");

    let mut bad_layout_row = header(VERSION, 120.0, 60.0, 10.0);
    bad_layout_row.extend(record(5, &columns(&[(0, 50.0), (1, 1.0)])));
    bad_layout_row.extend(plain(b"one"));
    bad_layout_row.extend(record(6, &[]));
    assert_eq!(push_error(&bad_layout_row), "row cell count mismatch");

    let mut unbounded = header(VERSION, 120.0, 60.0, 10.0);
    unbounded.extend(record(3, &0.0_f32.to_le_bytes()));
    let mut decoder = Decoder::default();
    decoder.push(&unbounded).unwrap();
    let chunk = plain(&[b'x'; 60_000]);
    let error = (0..300)
        .find_map(|_| decoder.push(&chunk).err())
        .expect("open stack must stop buffering");
    assert_eq!(error.to_string(), "block exceeds 16 MiB");

    let mut oversized = header(VERSION, 120.0, 60.0, 10.0);
    oversized.extend(record(3, &0.0_f32.to_le_bytes()));
    oversized.extend(plain("word ".repeat(100).as_bytes()));
    oversized.extend(record(4, &[]));
    assert_eq!(push_error(&oversized), "PageOverflow");
}

#[test]
fn given_bad_cell_slots_when_decoding_then_rejects() {
    let row = |slots: &[(u8, f32, &[u8])], cells: usize| {
        let mut bytes = record(5, &slotted(slots));
        for _ in 0..cells {
            bytes.extend(plain(b"x"));
        }
        bytes.extend(record(6, &[]));
        bytes
    };
    let span: &[u8] = &[4, 2, 0];
    for (body, error) in [
        (row(&[(1, 1.0, &[3])], 1), "invalid cell slot"),
        (row(&[(1, 1.0, &[4, 1, 0])], 1), "invalid row span"),
        (row(&[(1, 1.0, &[8])], 0), "InvalidLayout"),
        (row(&[(1, 1.0, &[16]), (1, 1.0, &[8])], 0), "InvalidLayout"),
        (
            [
                record(3, &0.0_f32.to_le_bytes()),
                row(&[(1, 1.0, span)], 1),
                record(4, &[]),
            ]
            .concat(),
            "InvalidLayout",
        ),
    ] {
        let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
        bytes.extend(body);
        assert_eq!(push_error(&bytes), error);
    }
}

#[test]
fn given_mutated_protocol_streams_when_decoding_then_never_panics() {
    let original = representative();
    let mut rng = crate::tests::XorShift(0x2545_f491_4f6c_dd1d);
    for _ in 0..3000 {
        let mut bytes = original.clone();
        rng.mutate(&mut bytes);
        let mut decoder = decoder();
        if decoder.push(&bytes).is_ok() {
            let _ = decoder.finish();
        }
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut bytes = (data.len() as u32).to_be_bytes().to_vec();
    bytes.extend(kind);
    bytes.extend(data);
    bytes.extend(crc32(&bytes[4..]).to_be_bytes());
    bytes
}

/// `scanlines` are the raw filtered rows, each starting with its filter byte.
fn png(width: u32, height: u32, color_type: u8, depth: u8, scanlines: &[u8]) -> Vec<u8> {
    let mut header = width.to_be_bytes().to_vec();
    header.extend(height.to_be_bytes());
    header.extend([depth, color_type, 0, 0, 0]);
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    bytes.extend(chunk(b"IHDR", &header));
    bytes.extend(chunk(
        b"IDAT",
        &miniz_oxide::deflate::compress_to_vec_zlib(scanlines, 6),
    ));
    bytes.extend(chunk(b"IEND", &[]));
    bytes
}

/// SOI, an optional Adobe APP14, and a frame header; the scan itself never reaches the parser.
fn jpeg(marker: u8, precision: u8, components: u8, adobe: bool) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xd8];
    if adobe {
        bytes.extend([0xff, 0xee, 0, 14]);
        bytes.extend(b"Adobe");
        bytes.extend([0, 100, 0, 0, 0, 0, 2]);
    }
    bytes.extend([
        0xff,
        marker,
        0,
        8 + 3 * components,
        precision,
        0,
        10,
        0,
        20,
        components,
    ]);
    bytes.extend([0, 0x11, 0].repeat(usize::from(components)));
    bytes.extend([0xff, 0xda, 0, 2, 0xff, 0xd9]);
    bytes
}

fn image_record(
    slot: u16,
    width: Option<f32>,
    height: Option<f32>,
    link: Option<&[u8]>,
) -> Vec<u8> {
    let mut payload = slot.to_le_bytes().to_vec();
    // Width, max width, height, max height; the max sizes stay `none`.
    for value in [width, None, height, None] {
        match value {
            Some(value) => {
                payload.push(1);
                payload.extend(value.to_le_bytes());
            }
            None => payload.push(0),
        }
    }
    match link {
        Some(link) => {
            payload.push(1);
            payload.extend(text(link));
        }
        None => payload.push(0),
    }
    payload.extend(text(b"Company logo"));
    payload.push(255);
    record(12, &payload)
}

fn decode_with_images(images: &[Vec<u8>], bytes: &[u8]) -> Result<lopdf::Document, String> {
    let mut decoder = Decoder::default();
    for image in images {
        decoder
            .add_image(image.clone())
            .map_err(|error| error.to_string())?;
    }
    decoder.push(bytes).map_err(|error| error.to_string())?;
    let pdf = decoder.finish().map_err(|error| error.to_string())?;
    Ok(lopdf::Document::load_mem(&pdf).unwrap())
}

fn first_xobject(pdf: &lopdf::Document) -> &lopdf::Stream {
    let page = pdf.get_pages()[&1];
    let resources = pdf.get_page_resources(page).unwrap().0.unwrap();
    let xobjects = resources.get(b"XObject").unwrap().as_dict().unwrap();
    let reference = xobjects.get(b"Im1").unwrap().as_reference().unwrap();
    pdf.get_object(reference).unwrap().as_stream().unwrap()
}

#[test]
fn given_jpeg_frame_headers_when_parsing_then_reads_size_components_and_adobe_inversion() {
    let cmyk = crate::Image::parse(jpeg(0xc2, 8, 4, true)).unwrap();
    assert_eq!((cmyk.width, cmyk.height), (20, 10));
    assert!(matches!(cmyk.color_space, crate::image::ColorSpace::Cmyk));
    assert!(matches!(
        cmyk.encoding,
        crate::image::Encoding::Dct { inverted: true }
    ));
    let rgb = crate::Image::parse(jpeg(0xc0, 8, 3, false)).unwrap();
    assert!(matches!(
        rgb.encoding,
        crate::image::Encoding::Dct { inverted: false }
    ));
    let reject = |bytes| crate::Image::parse(bytes).err().unwrap();
    assert!(reject(jpeg(0xc1, 12, 3, false)).starts_with("12-bit JPEG"));
    assert!(reject(jpeg(0xc3, 8, 3, false)).starts_with("lossless"));
}

#[test]
fn given_an_rgb_png_when_rendered_then_passes_the_idat_through_with_predictor_15() {
    let logo = png(
        2,
        2,
        2,
        8,
        &[0, 255, 0, 0, 0, 255, 0, 1, 0, 0, 255, 9, 9, 9],
    );
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    bytes.extend(image_record(0, Some(40.0), None, None));
    bytes.extend(record(255, &[]));
    let pdf = decode_with_images(std::slice::from_ref(&logo), &bytes).unwrap();
    let image = first_xobject(&pdf);
    let parms = image.dict.get(b"DecodeParms").unwrap().as_dict().unwrap();
    assert_eq!(parms.get(b"Predictor").unwrap().as_i64().unwrap(), 15);
    assert_eq!(parms.get(b"Colors").unwrap().as_i64().unwrap(), 3);
    assert_eq!(parms.get(b"Columns").unwrap().as_i64().unwrap(), 2);
    assert_eq!(image.dict.get(b"Width").unwrap().as_i64().unwrap(), 2);
    let idat_start = logo.windows(4).position(|w| w == b"IDAT").unwrap() + 4;
    assert_eq!(
        image.content,
        logo[idat_start..idat_start + image.content.len()]
    );
    let content = String::from_utf8(pdf.get_page_content(pdf.get_pages()[&1])).unwrap();
    assert!(
        content.contains("q\n40 0 0 40 10 10 cm\n/Im1 Do\nQ"),
        "{content}"
    );
}

#[test]
fn given_an_rgba_png_when_rendered_then_splits_the_alpha_into_an_smask() {
    // Two pixels, one row: Sub-filtered so the second pixel is a delta from the first.
    let logo = png(2, 1, 6, 8, &[1, 10, 20, 30, 255, 5, 5, 5, 1]);
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    bytes.extend(image_record(0, None, Some(20.0), None));
    bytes.extend(record(255, &[]));
    let pdf = decode_with_images(&[logo], &bytes).unwrap();
    let image = first_xobject(&pdf);
    assert_eq!(
        image.decompressed_content().unwrap(),
        [10, 20, 30, 15, 25, 35]
    );
    let mask = pdf
        .get_object(image.dict.get(b"SMask").unwrap().as_reference().unwrap())
        .unwrap()
        .as_stream()
        .unwrap();
    assert_eq!(mask.decompressed_content().unwrap(), [255, 0]);
    assert_eq!(
        mask.dict.get(b"ColorSpace").unwrap().as_name().unwrap(),
        b"DeviceGray"
    );
    let content = String::from_utf8(pdf.get_page_content(pdf.get_pages()[&1])).unwrap();
    assert!(content.contains("40 0 0 20 10 30 cm"), "{content}");
}

#[test]
fn given_unsupported_pngs_when_registering_then_rejects_naming_the_feature() {
    let mut decoder = Decoder::default();
    let sixteen = png(1, 1, 2, 16, &[0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(
        decoder.add_image(sixteen).unwrap_err().to_string(),
        "16-bit PNG is not supported; save it with 8 bits per channel"
    );
    let mut interlaced = png(1, 1, 2, 8, &[0, 0, 0, 0]);
    let ihdr = interlaced.windows(4).position(|w| w == b"IHDR").unwrap();
    interlaced[ihdr + 4 + 12] = 1;
    assert!(decoder
        .add_image(interlaced)
        .unwrap_err()
        .to_string()
        .starts_with("interlaced"));
    assert!(decoder
        .add_image(b"GIF89a".to_vec())
        .unwrap_err()
        .to_string()
        .starts_with("unsupported image format"));
}

#[test]
fn given_a_png_whose_data_inflates_past_its_dimensions_when_registering_then_stops_at_their_size() {
    // Without its checksum the stream is invalid, so only an inflater that stops early sees the size.
    let mut stream = miniz_oxide::deflate::compress_to_vec_zlib(&[0; 1 << 16], 6);
    stream.truncate(stream.len() - 4);
    let valid = png(1, 1, 6, 8, &[0; 5]);
    let idat = valid.windows(4).position(|w| w == b"IDAT").unwrap() - 4;
    let mut bomb = valid[..idat].to_vec();
    bomb.extend(chunk(b"IDAT", &stream));
    bomb.extend(chunk(b"IEND", &[]));
    assert_eq!(
        Decoder::default().add_image(bomb).unwrap_err().to_string(),
        "PNG image data does not match its dimensions"
    );
}

/// A vector drawing: a box size, then each path's flags, matrix, paints, and segment bytes.
fn drawing(width: f32, height: f32, paths: &[(u8, &[u8], &[u8])]) -> Vec<u8> {
    let mut bytes = b"FPDV".to_vec();
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    for (flags, paint, segments) in paths {
        bytes.push(*flags);
        for value in [1.0 / width, 0.0, 0.0, -1.0 / height, 0.0, 1.0] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(*paint);
        bytes.extend(*segments);
    }
    bytes
}

fn segments(items: &[(u8, &[f32])]) -> Vec<u8> {
    let mut bytes = (items.len() as u32).to_le_bytes().to_vec();
    for (op, points) in items {
        bytes.push(*op);
        for value in *points {
            bytes.extend(value.to_le_bytes());
        }
    }
    bytes
}

fn stroke(width: f32, dashes: &[f32]) -> Vec<u8> {
    let mut bytes = vec![0, 0, 255, 255];
    bytes.extend(width.to_le_bytes());
    bytes.extend(4.0_f32.to_le_bytes());
    bytes.push(dashes.len() as u8);
    for value in dashes.iter().chain([&0.0]) {
        bytes.extend(value.to_le_bytes());
    }
    bytes
}

#[test]
fn given_a_vector_drawing_when_rendered_then_paints_a_form_clipped_to_the_box() {
    let square = segments(&[
        (0, &[0.0, 0.0]),
        (1, &[30.0, 0.0]),
        (2, &[30.0, 10.0, 20.0, 20.0, 0.0, 20.0]),
        (3, &[]),
    ]);
    let line = segments(&[(0, &[0.0, 10.0]), (1, &[30.0, 10.0])]);
    // Even-odd fill at alpha 0x80; a round-capped, bevel-joined, dashed stroke.
    let logo = drawing(
        30.0,
        20.0,
        &[
            (3, &[255, 0, 0, 0x80], &square),
            (4 | 1 << 3 | 2 << 5, &stroke(2.0, &[3.0, 1.0]), &line),
        ],
    );
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    bytes.extend(image_record(0, Some(60.0), None, None));
    bytes.extend(record(255, &[]));
    let pdf = decode_with_images(&[logo], &bytes).unwrap();
    let form = first_xobject(&pdf);
    assert_eq!(
        form.dict.get(b"Subtype").unwrap().as_name().unwrap(),
        b"Form"
    );
    let bbox = form.dict.get(b"BBox").unwrap().as_array().unwrap();
    let bbox: Vec<f32> = bbox.iter().map(|value| value.as_float().unwrap()).collect();
    assert_eq!(bbox, [0.0, 0.0, 1.0, 1.0]);
    let states = form.dict.get(b"Resources").unwrap().as_dict().unwrap();
    let state = states.get(b"ExtGState").unwrap().as_dict().unwrap();
    let alpha = state.get(b"G80FF").unwrap().as_dict().unwrap();
    assert_eq!(alpha.get(b"ca").unwrap().as_float().unwrap(), 128.0 / 255.0);
    assert_eq!(
        String::from_utf8(form.decompressed_content().unwrap()).unwrap(),
        "q\n0.033333335 0 0 -0.05 0 1 cm\n1 0 0 rg\n/G80FF gs\n0 0 m\n30 0 l\n30 10 20 20 0 20 c\nh\nf*\nQ\n\
         q\n0.033333335 0 0 -0.05 0 1 cm\n0 0 1 RG\n2 w\n1 J\n2 j\n4 M\n[3 1] 0 d\n0 10 m\n30 10 l\nS\nQ"
    );
    let content = String::from_utf8(pdf.get_page_content(pdf.get_pages()[&1])).unwrap();
    assert!(
        content.contains("q\n60 0 0 40 10 10 cm\n/Im1 Do\nQ"),
        "{content}"
    );
}

#[test]
fn given_malformed_vector_drawings_when_registering_then_rejects() {
    let fill = [0, 0, 0, 255];
    let moved = segments(&[(0, &[0.0, 0.0]), (1, &[1.0, 1.0])]);
    for bytes in [
        drawing(0.0, 10.0, &[(1, &fill, &moved)]),
        drawing(10.0, 10.0, &[(1, &fill, &segments(&[(1, &[1.0, 1.0])]))]),
        drawing(
            10.0,
            10.0,
            &[(
                1,
                &fill,
                &segments(&[(0, &[0.0, 0.0]), (3, &[]), (1, &[1.0, 1.0])]),
            )],
        ),
        drawing(10.0, 10.0, &[(2, &[], &moved)]),
        drawing(
            10.0,
            10.0,
            &[(1, &fill, &segments(&[(0, &[f32::NAN, 0.0])]))],
        ),
        drawing(10.0, 10.0, &[(4, &stroke(0.0, &[]), &moved)]),
        drawing(10.0, 10.0, &[(4, &stroke(1.0, &[0.0, 0.0]), &moved)]),
        drawing(10.0, 10.0, &[(1, &fill, &segments(&[]))]),
        drawing(10.0, 10.0, &[(1, &fill, &moved[..moved.len() - 1])]),
    ] {
        assert_eq!(
            Decoder::default().add_image(bytes).unwrap_err().to_string(),
            "invalid vector drawing"
        );
    }
}

#[test]
fn given_fonts_over_64_mib_when_registering_then_rejects_before_parsing() {
    let mut decoder = Decoder::default();
    assert_eq!(
        decoder
            .add_font(vec![0; MAX_FONT_BYTES + 1])
            .unwrap_err()
            .to_string(),
        "fonts exceed 64 MiB per render"
    );
}

#[test]
fn given_rejected_fonts_and_images_at_the_cap_when_registering_more_then_their_bytes_do_not_count()
{
    let mut decoder = Decoder::default();
    assert!(decoder.add_font(vec![0; MAX_FONT_BYTES]).is_err());
    assert!(decoder.add_image(vec![0; MAX_IMAGE_BYTES]).is_err());
    let font = include_bytes!("../../../../packages/flashpdf/test/fixtures/Abel-Regular.ttf");
    decoder.add_font(font.to_vec()).unwrap();
    decoder.add_image(png(1, 1, 0, 8, &[0, 128])).unwrap();
}

#[test]
fn given_page_breaks_past_the_page_cap_when_decoding_then_rejects_naming_the_cap() {
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    bytes.extend(plain(b"x"));
    for _ in 1..MAX_PAGES {
        bytes.extend(record(7, &[]));
        bytes.extend(plain(b"x"));
    }
    let mut decoder = Decoder::default();
    decoder.push(&bytes).unwrap();
    assert_eq!(
        decoder
            .push(&[record(7, &[]), plain(b"x")].concat())
            .unwrap_err()
            .to_string(),
        "document exceeds 10000 pages; split it across several renders"
    );
}

#[test]
fn given_a_top_level_table_over_the_block_cap_when_decoding_then_streams_its_rows() {
    let row = [
        record(5, &columns(&[(1, 1.0)])),
        plain(b"a"),
        record(6, &[]),
    ]
    .concat();
    // Each row buffers at least three commands, so these rows together exceed the cap.
    let rows = MAX_BLOCK_BYTES / (3 * size_of::<OwnedCommand>()) + 1;
    let mut decoder = Decoder::default();
    decoder.push(&header(VERSION, 595.0, 842.0, 36.0)).unwrap();
    decoder.push(&record(10, &table_start(1))).unwrap();
    for _ in 0..=rows {
        decoder.push(&row).unwrap();
    }
    decoder.push(&record(11, &[])).unwrap();
    decoder.push(&record(255, &[])).unwrap();
    decoder.finish().unwrap();
}

#[test]
fn given_a_painted_box_around_a_table_over_the_block_cap_when_decoding_then_buffers_about_a_page() {
    let row = [
        record(5, &columns(&[(1, 1.0)])),
        plain(b"a"),
        record(6, &[]),
    ]
    .concat();
    let rows = MAX_BLOCK_BYTES / (3 * size_of::<OwnedCommand>()) + 1;
    let mut decoder = Decoder::default();
    decoder.push(&header(VERSION, 595.0, 842.0, 36.0)).unwrap();
    decoder.push(&box_start()).unwrap();
    decoder.push(&record(10, &table_start(1))).unwrap();
    for _ in 0..=rows {
        decoder.push(&row).unwrap();
        assert!(decoder.commands.block_bytes < 1024);
    }
    decoder.push(&record(11, &[])).unwrap();
    decoder.push(&record(18, &[])).unwrap();
    decoder.push(&record(255, &[])).unwrap();
    decoder.finish().unwrap();
}

#[test]
fn given_a_page_side_over_14400_units_when_decoding_then_rejects() {
    assert_eq!(
        push_error(&header(VERSION, 14_401.0, 60.0, 10.0)),
        "invalid page size"
    );
    assert_eq!(
        push_error(&header(VERSION, 120.0, f32::MAX, 10.0)),
        "invalid page size"
    );
}

#[test]
fn given_page_margins_that_leave_no_width_or_no_height_when_decoding_then_rejects() {
    for margin in [[0.0, 60.0, 0.0, 60.0], [30.0, 0.0, 30.0, 0.0]] {
        assert_eq!(
            push_error(&page_header(VERSION, 120.0, 60.0, margin)),
            "InvalidPage"
        );
    }
    let narrow = page_header(VERSION, 120.0, 60.0, [29.0, 0.0, 30.0, 119.0]);
    Decoder::default().push(&narrow).unwrap();
}

#[test]
fn given_a_link_run_that_wraps_when_rendered_then_emits_one_annotation_per_line() {
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    bytes.extend(linked_paragraph(
        0,
        &[b"https://example.com/invoices/42"],
        &[(
            0,
            10.0,
            [0, 0, 0xee],
            2,
            Some(0),
            b"open the invoice portal",
        )],
    ));
    bytes.extend(record(255, &[]));
    let pdf = decode_with_images(&[], &bytes).unwrap();
    let page = pdf.get_pages()[&1];
    let content = String::from_utf8(pdf.get_page_content(page)).unwrap();
    let lines = content.matches("BT\n").count();
    assert!(lines >= 2, "{content}");
    assert_eq!(content.matches(" re\nf\n").count(), lines, "{content}");
    let annotations = pdf.get_page_annotations(page).unwrap();
    assert_eq!(annotations.len(), lines);
    for annotation in annotations {
        assert_eq!(
            annotation.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Link"
        );
        let action = annotation.get(b"A").unwrap().as_dict().unwrap();
        assert_eq!(
            action.get(b"URI").unwrap().as_str().unwrap(),
            b"https://example.com/invoices/42"
        );
    }
}

/// The page, left, and top of the `/Dests` entry for `name`.
fn destination(pdf: &lopdf::Document, name: &[u8]) -> (lopdf::ObjectId, f32, f32) {
    let names = pdf.catalog().unwrap().get(b"Names").unwrap();
    let tree = names.as_dict().unwrap().get(b"Dests").unwrap();
    let entries = tree.as_dict().unwrap().get(b"Names").unwrap();
    let entries = entries.as_array().unwrap();
    let index = (entries.chunks(2))
        .position(|entry| entry[0].as_str().unwrap() == name)
        .unwrap();
    let reference = entries[index * 2 + 1].as_reference().unwrap();
    let dest = pdf.get_object(reference).unwrap().as_array().unwrap();
    assert_eq!(dest[1].as_name().unwrap(), b"XYZ");
    assert!(matches!(dest[4], lopdf::Object::Null), "{dest:?}");
    (
        dest[0].as_reference().unwrap(),
        dest[2].as_float().unwrap(),
        dest[3].as_float().unwrap(),
    )
}

#[test]
fn given_an_anchor_before_a_block_that_moves_to_the_next_page_when_decoding_then_the_destination_follows_it_and_a_link_goes_there(
) {
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    bytes.extend(linked_paragraph(
        0,
        &[b"#end"],
        &[(0, 10.0, [0, 0, 0xee], 2, Some(0), b"Jump")],
    ));
    bytes.extend(record(2, &30.0_f32.to_le_bytes()));
    bytes.extend(anchor("end", 0, ""));
    bytes.extend(plain(b"Target"));
    bytes.extend(record(255, &[]));
    let pdf = decode_with_images(&[], &bytes).unwrap();
    let pages = pdf.get_pages();
    assert_eq!(destination(&pdf, b"end"), (pages[&2], 10.0, 50.0));
    let link = pdf.get_page_annotations(pages[&1]).unwrap()[0];
    let action = link.get(b"A").unwrap().as_dict().unwrap();
    assert_eq!(action.get(b"S").unwrap().as_name().unwrap(), b"GoTo");
    assert_eq!(action.get(b"D").unwrap().as_str().unwrap(), b"end");
}

#[test]
fn given_headings_when_decoding_then_nests_bookmarks_by_level_and_opens_the_outline_panel() {
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    for (level, title) in [
        (1, "Invoice"),
        (2, "Items"),
        (3, "Row"),
        (2, "Totals \u{3a9}"),
        (1, "Notes"),
    ] {
        bytes.extend(anchor("", level, title));
        bytes.extend(plain(b"h"));
    }
    bytes.extend(record(255, &[]));
    let pdf = decode_with_images(&[], &bytes).unwrap();
    let catalog = pdf.catalog().unwrap();
    assert_eq!(
        catalog.get(b"PageMode").unwrap().as_name().unwrap(),
        b"UseOutlines"
    );
    let item = |object: &lopdf::Object| pdf.get_dictionary(object.as_reference().unwrap()).unwrap();
    let root = item(catalog.get(b"Outlines").unwrap());
    assert_eq!(root.get(b"Count").unwrap().as_i64().unwrap(), 5);
    let (invoice, notes) = (
        item(root.get(b"First").unwrap()),
        item(root.get(b"Last").unwrap()),
    );
    assert_eq!(invoice.get(b"Title").unwrap().as_str().unwrap(), b"Invoice");
    assert_eq!(invoice.get(b"Count").unwrap().as_i64().unwrap(), 3);
    assert_eq!(notes.get(b"Prev").unwrap(), root.get(b"First").unwrap());
    assert!(notes.get(b"Next").is_err());
    let (items, totals) = (
        item(invoice.get(b"First").unwrap()),
        item(invoice.get(b"Last").unwrap()),
    );
    assert_eq!(items.get(b"Next").unwrap(), invoice.get(b"Last").unwrap());
    assert_eq!(items.get(b"Count").unwrap().as_i64().unwrap(), 1);
    assert_eq!(
        item(items.get(b"First").unwrap())
            .get(b"Title")
            .unwrap()
            .as_str()
            .unwrap(),
        b"Row"
    );
    // Not PDFDocEncoding, so UTF-16BE with a byte order mark.
    assert_eq!(
        totals.get(b"Title").unwrap().as_str().unwrap(),
        [0xfe, 0xff, 0, b'T', 0, b'o', 0, b't', 0, b'a', 0, b'l', 0, b's', 0, b' ', 3, 0xa9]
    );
}

#[test]
fn given_bad_anchors_and_links_when_decoding_then_rejects() {
    let error = |records: &[Vec<u8>]| {
        let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
        bytes.extend(records.concat());
        Decoder::default().push(&bytes).unwrap_err().to_string()
    };
    for (record, message) in [
        (anchor("a b", 0, ""), "invalid anchor name"),
        (anchor(&"a".repeat(256), 0, ""), "invalid anchor name"),
        (anchor("", 7, "Title"), "invalid bookmark"),
        (anchor("", 1, ""), "invalid bookmark"),
        (anchor("", 0, "Title"), "invalid bookmark"),
        (anchor("", 1, &"t".repeat(1021)), "invalid bookmark"),
    ] {
        assert_eq!(error(&[record]), message);
    }
    let twice = [anchor("a", 0, ""), anchor("a", 0, "")];
    assert_eq!(error(&twice), "invalid anchor name: already used");
    let many: Vec<Vec<u8>> = (0..=10_000)
        .map(|index| anchor(&format!("a{index}"), 0, ""))
        .collect();
    assert_eq!(error(&many), "invalid anchor count");
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    bytes.extend(linked_paragraph(
        0,
        &[b"#missing"],
        &[(0, 10.0, [0, 0, 0xee], 2, Some(0), b"Jump")],
    ));
    bytes.extend(record(255, &[]));
    assert_eq!(
        decode_with_images(&[], &bytes).unwrap_err(),
        "UnknownDestination"
    );
}

#[test]
fn given_anchors_in_a_row_and_a_gapped_stack_when_decoding_then_they_take_no_cell_and_no_gap() {
    let page_content = |anchors: bool| {
        let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
        let mark = |name: &str| {
            if anchors {
                anchor(name, 0, "")
            } else {
                Vec::new()
            }
        };
        bytes.extend(record(5, &columns(&[(0, 50.0), (1, 1.0)])));
        bytes.extend(plain(b"a"));
        bytes.extend(mark("second"));
        bytes.extend(plain(b"b"));
        bytes.extend(record(6, &[]));
        bytes.extend(record(3, &5.0_f32.to_le_bytes()));
        bytes.extend(mark("first"));
        bytes.extend(plain(b"c"));
        bytes.extend(mark("between"));
        bytes.extend(plain(b"d"));
        bytes.extend(record(4, &[]));
        bytes.extend(record(255, &[]));
        decode_with_images(&[], &bytes).unwrap()
    };
    let (plain, anchored) = (page_content(false), page_content(true));
    let page = anchored.get_pages()[&1];
    assert_eq!(
        plain.get_page_content(plain.get_pages()[&1]),
        anchored.get_page_content(page)
    );
    assert_eq!(destination(&anchored, b"second"), (page, 60.0, 50.0));
    let (first, between) = (
        destination(&anchored, b"first").2,
        destination(&anchored, b"between").2,
    );
    assert!(first - between > 5.0, "{first} {between}");
}

#[test]
fn given_an_anchor_in_a_repeated_table_header_when_the_table_spans_pages_then_the_destination_is_its_first_fragment(
) {
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    bytes.extend(record(10, &table_start(1)));
    for index in 0..12 {
        bytes.extend(record(5, &columns(&[(1, 1.0)])));
        if index == 0 {
            bytes.extend(anchor("head", 0, ""));
        }
        bytes.extend(plain(if index == 0 { b"head" } else { b"row" }));
        bytes.extend(record(6, &[]));
    }
    bytes.extend(record(11, &[]));
    bytes.extend(record(255, &[]));
    let pdf = decode_with_images(&[], &bytes).unwrap();
    assert!(pdf.get_pages().len() > 2);
    assert_eq!(destination(&pdf, b"head").0, pdf.get_pages()[&1]);
}

/// An Attachment record with an empty description.
fn attachment(slot: u16, relationship: u8, name: &str, mime: &str) -> Vec<u8> {
    let mut payload = slot.to_le_bytes().to_vec();
    payload.push(relationship);
    payload.extend(text(name.as_bytes()));
    payload.extend(text(mime.as_bytes()));
    payload.extend(text(b""));
    record(21, &payload)
}

#[test]
fn given_pdfa_and_attachment_records_when_decoding_then_rejects_what_the_output_cannot_carry() {
    let error = |files: &[&[u8]], records: &[Vec<u8>]| {
        let mut decoder = Decoder::default();
        for file in files {
            decoder.add_attachment(file.to_vec()).unwrap();
        }
        let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
        bytes.extend(records.concat());
        decoder.push(&bytes).unwrap_err().to_string()
    };
    let pdfa = record(20, &[0]);
    let facturx = record(20, &[4, 0, 0]);

    for slot in [0, 4, 11] {
        let total = paragraph(0, &[(slot, 10.0, [0, 0, 0], 0, b"Total")]);
        assert!(error(&[], &[pdfa.clone(), total]).starts_with("PDF/A embeds every font"));
    }
    assert_eq!(
        error(&[b"Total: 42"], std::slice::from_ref(&facturx)),
        "invalid Factur-X XML"
    );
    assert_eq!(
        error(&[b"<x/>"], &[record(20, &[9, 0, 0])]),
        "invalid PDF/A profile"
    );
    // PDF/A-2 rejects an attachment whichever record comes first.
    let file = attachment(0, 0, "a.txt", "text/plain");
    for records in [
        [record(20, &[7]), file.clone()],
        [file.clone(), record(20, &[8])],
    ] {
        assert!(error(&[b"x"], &records).starts_with("PDF/A-2 embeds only files that are PDF/A"));
    }
    assert_eq!(
        error(
            &[b"<x/>", b"x"],
            &[facturx, attachment(1, 1, "factur-x.xml", "text/xml")]
        ),
        "duplicate attachment name"
    );
    for (record, message) in [
        (
            attachment(0, 0, "a/b.txt", "text/plain"),
            "invalid attachment name",
        ),
        (
            attachment(0, 0, "", "text/plain"),
            "invalid attachment name",
        ),
        (
            attachment(0, 0, "a.txt", "text plain"),
            "invalid attachment MIME type",
        ),
        (
            attachment(0, 5, "a.txt", "text/plain"),
            "invalid attachment relationship",
        ),
        (
            attachment(1, 0, "a.txt", "text/plain"),
            "invalid attachment slot",
        ),
    ] {
        assert_eq!(error(&[b"x"], &[record]), message);
    }
    let reused = [
        attachment(0, 0, "a", "text/plain"),
        attachment(0, 0, "b", "text/plain"),
    ];
    assert_eq!(error(&[b"x"], &reused), "invalid attachment slot");
    let late = [record(2, &5.0_f32.to_le_bytes()), pdfa.clone()];
    assert_eq!(error(&[], &late), "invalid nesting");

    // CMYK rejects whether the image registers before or after the PDF/A record.
    let cmyk = jpeg(0xc0, 8, 4, true);
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    bytes.extend(&pdfa);
    let mut decoder = Decoder::default();
    decoder.add_image(cmyk.clone()).unwrap();
    let message = decoder.push(&bytes).unwrap_err().to_string();
    assert!(
        message.contains("CMYK JPEGs are not supported"),
        "{message}"
    );
    let mut decoder = Decoder::default();
    decoder.push(&bytes).unwrap();
    assert!(decoder.add_image(cmyk).is_err());

    let mut decoder = Decoder::default();
    decoder.add_attachment(vec![0; 1024]).unwrap();
    let message = decoder
        .add_attachment(vec![0; super::MAX_ATTACHMENT_BYTES])
        .unwrap_err()
        .to_string();
    assert_eq!(message, "attachments exceed 64 MiB per render");
}

#[test]
fn given_a_footer_when_decoding_then_repeats_resolved_page_numbers() {
    let mut bytes = header(VERSION, 595.0, 842.0, 36.0);
    bytes.extend(record(8, &[0]));
    bytes.extend(record(3, &0.0_f32.to_le_bytes()));
    bytes.extend(plain("Page \u{1e} of \u{1f}".as_bytes()));
    bytes.extend(record(4, &[]));
    bytes.extend(plain(b"First"));
    bytes.extend(record(7, &[]));
    bytes.extend(plain(b"Second"));
    bytes.extend(record(255, &[]));

    let pdf = lopdf::Document::load_mem(&decode(&bytes).unwrap()).unwrap();
    assert_eq!(pdf.extract_text(&[1]).unwrap().trim(), "First\nPage 1 of 2");
    assert_eq!(
        pdf.extract_text(&[2]).unwrap().trim(),
        "Second\nPage 2 of 2"
    );
}

#[test]
fn given_nested_buffered_frames_when_closing_then_each_end_completes_only_its_own_frame() {
    let mut bytes = header(VERSION, 100.0, 100.0, 10.0);
    bytes.extend(record(3, &0.0_f32.to_le_bytes()));
    bytes.extend(record(5, &columns(&[(0, 80.0)])));
    bytes.extend(record(3, &0.0_f32.to_le_bytes()));
    bytes.extend(paragraph(0, &[(0, 10.0, [0, 0, 0], 0, b"nested")]));
    bytes.extend(record(4, &[]));
    bytes.extend(record(6, &[]));
    bytes.extend(record(4, &[]));
    bytes.extend(record(255, &[]));
    let mut decoder = Decoder::default();
    decoder.push(&bytes).unwrap();
    let pdf = lopdf::Document::load_mem(&decoder.finish().unwrap()).unwrap();
    assert_eq!(pdf.extract_text(&[1]).unwrap().trim(), "nested");
}

#[test]
fn given_a_rounded_box_crossing_a_page_when_decoding_then_keeps_its_children_together() {
    let mut bytes = header(VERSION, 100.0, 60.0, 10.0);
    bytes.extend(record(2, &30.0_f32.to_le_bytes()));
    let mut radius = vec![255, 0, 1, 0];
    for _ in 0..8 {
        radius.extend(2.0_f32.to_le_bytes());
    }
    bytes.extend(decorated_box(0.0, &radius));
    bytes.extend(plain(b"one"));
    bytes.extend(plain(b"two"));
    bytes.extend(record(18, &[]));
    bytes.extend(record(255, &[]));
    let pdf = lopdf::Document::load_mem(&decode(&bytes).unwrap()).unwrap();
    assert_eq!(pdf.get_pages().len(), 2);
    assert!(crate::tests::page_text(&pdf, 1).is_empty());
    assert_eq!(crate::tests::page_text(&pdf, 2), "one\ntwo");
}

/// A Setup record with no page options: the page is `@page`'s, or A4 with 36pt margins.
fn elements(sheets: &[&str]) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.extend(VERSION.to_le_bytes());
    for sheet in sheets {
        bytes.extend(record(27, &[&[0], sheet.as_bytes()].concat()));
    }
    bytes.extend(record(28, &[0, 0, 0, 0]));
    bytes
}

/// An Open record of the tag at `tag` in `TAGS` with `flags`, no id or classes, then `rest`.
fn open(tag: u8, flags: u16, rest: &[u8]) -> Vec<u8> {
    let mut payload = vec![tag];
    payload.extend(flags.to_le_bytes());
    payload.extend(0_u16.to_le_bytes());
    payload.extend(rest);
    record(30, &payload)
}

fn element_text(value: &str) -> Vec<u8> {
    record(31, &[&[0], value.as_bytes()].concat())
}

const MAIN: u8 = 0;
const DIV: u8 = 1;
const P: u8 = 18;
const IMG: u8 = 51;

#[test]
fn given_element_records_with_a_stylesheet_header_and_staged_image_when_decoding_then_compiles_the_page(
) {
    let mut bytes = elements(&[".a { color: red }", "@page { size: A5 }"]);
    bytes.extend(record(29, &[1]));
    bytes.extend(open(P, 0, &[]));
    bytes.extend(element_text("Top"));
    bytes.extend(record(32, &[]));
    bytes.extend(record(29, &[0]));
    bytes.extend(open(MAIN, 0, &[]));
    let mut styled = vec![P];
    styled.extend(0_u16.to_le_bytes());
    styled.extend(1_u16.to_le_bytes());
    styled.extend(text(b"a"));
    bytes.extend(record(30, &styled));
    bytes.extend(element_text("Hello"));
    bytes.extend(record(32, &[]));
    bytes.extend(open(IMG, 1 << 10, &0_u16.to_le_bytes()));
    bytes.extend(record(32, &[]));
    bytes.extend(record(32, &[]));
    bytes.extend(record(255, &[]));
    let mut decoder = Decoder::default();
    decoder.stage_image(png(1, 1, 0, 8, &[0, 128])).unwrap();
    decoder.push(&bytes).unwrap();
    let pdf = lopdf::Document::load_mem(&decoder.finish().unwrap()).unwrap();
    assert_eq!(pdf.extract_text(&[1]).unwrap().trim(), "Hello\nTop");
    let page = pdf.get_pages()[&1];
    let media = pdf.get_dictionary(page).unwrap().get(b"MediaBox").unwrap();
    assert_eq!(media.as_array().unwrap()[2].as_float().unwrap(), 420.0);
}

#[test]
fn given_element_records_outside_the_protocol_when_decoding_then_rejects() {
    let body = |records: &[Vec<u8>]| {
        let mut bytes = elements(&[]);
        bytes.extend(record(29, &[0]));
        bytes.extend(records.concat());
        bytes.extend(record(255, &[]));
        bytes
    };
    let close = record(32, &[]);
    let deep: Vec<Vec<u8>> = (0..66).map(|_| open(DIV, 0, &[])).collect();
    let mut late = elements(&[]);
    late.extend(record(2, &0.0_f32.to_le_bytes()));
    let mut early = MAGIC.to_vec();
    early.extend(VERSION.to_le_bytes());
    early.extend(open(P, 0, &[]));
    let mut reordered = elements(&[]);
    reordered.extend(record(29, &[0]));
    reordered.extend(record(29, &[1]));
    for (bytes, message) in [
        (body(&[open(200, 0, &[]), close.clone()]), "invalid tag"),
        (body(&[open(DIV, 1 << 2, &text(b"https://a.b/")), close.clone()]), "invalid element attributes"),
        (body(&[open(IMG, 1 << 10, &5_u16.to_le_bytes()), close.clone()]), "unknown image"),
        (body(&deep), "nesting exceeds 64"),
        (body(std::slice::from_ref(&close)), "invalid nesting"),
        (body(&[open(DIV, 0, &[])]), "invalid nesting"),
        (body(&[record(31, &[0, 0xff])]), "invalid UTF-8"),
        (body(&[record(31, &[0, 7])]), "unsupported control character"),
        (late, "invalid nesting"),
        (early, "invalid nesting"),
        (reordered, "invalid nesting"),
        (elements(&["@page { size: B5 }"]), "unsupported page size: B5 (use A3, A4, A5, Letter, Legal, or a width and height) in @page at 1:1"),
        (
            body(&[open(P, 1 << 11, &text(b"color: nope")), element_text("x"), close.clone()]),
            "unsupported color: nope on <p>",
        ),
    ] {
        let mut decoder = Decoder::default();
        let error = decoder.push(&bytes).err().map(|error| error.to_string());
        assert_eq!(error.as_deref(), Some(message));
    }
}

#[test]
fn given_a_strict_stylesheet_record_when_decoding_then_rejects_a_rule_no_element_matches() {
    let header = [MAGIC.as_slice(), &VERSION.to_le_bytes()].concat();
    let sheet = |code: u8| {
        [
            header.clone(),
            record(code, &[&[0], ".unused { color: nope }".as_bytes()].concat()),
        ]
        .concat()
    };
    assert!(Decoder::default().push(&sheet(27)).is_ok());
    let error = Decoder::default()
        .push(&sheet(34))
        .err()
        .map(|error| error.to_string());
    assert_eq!(
        error.as_deref(),
        Some("unsupported color: nope in selector \".unused\" at 1:1")
    );
}

/// A body in a family "Flex" of the Roboto Flex fixture, its regular file only: a paragraph
/// styled `style`, then one at the defaults.
fn flex(style: &str) -> Result<Vec<u8>, String> {
    let font = include_bytes!("../../../../packages/flashpdf/test/fixtures/RobotoFlex-subset.ttf");
    let mut decoder = Decoder::default();
    let slot = decoder.add_font(font.to_vec()).unwrap();
    let mut bytes = MAGIC.to_vec();
    bytes.extend(VERSION.to_le_bytes());
    bytes.extend(record(
        26,
        &[text(b"Flex"), vec![slot, 255, 255, 255]].concat(),
    ));
    bytes.extend(record(28, &[0, 0, 0, 0]));
    bytes.extend(record(29, &[0]));
    bytes.extend(open(MAIN, 1 << 11, &text(b"font-family: Flex")));
    bytes.extend(open(P, 1 << 11, &text(style.as_bytes())));
    bytes.extend(element_text("Hav"));
    bytes.extend(record(32, &[]));
    bytes.extend(open(P, 0, &[]));
    bytes.extend(element_text("Hav"));
    bytes.extend(record(32, &[]));
    bytes.extend(record(32, &[]));
    bytes.extend(record(255, &[]));
    decoder.push(&bytes).map_err(|error| error.to_string())?;
    decoder.finish().map_err(|error| error.to_string())
}

fn base_fonts(pdf: &[u8]) -> Vec<String> {
    let pdf = lopdf::Document::load_mem(pdf).unwrap();
    let mut names: Vec<String> = pdf
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dict| {
            dict.get(b"Subtype")
                .is_ok_and(|kind| kind.as_name().is_ok_and(|name| name == b"Type0"))
        })
        .map(|dict| {
            let name = dict.get(b"BaseFont").unwrap().as_name().unwrap();
            String::from_utf8_lossy(&name[7..]).into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn given_a_variable_family_without_a_bold_file_when_text_asks_for_bold_condensed_then_embeds_an_instance_named_by_its_axes_beside_the_default(
) {
    let pdf = flex("font-weight: bold; font-stretch: condensed").unwrap();
    assert_eq!(
        base_fonts(&pdf),
        ["RobotoFlex-Regular", "RobotoFlex-Regular_700wght_75wdth"]
    );
    assert_eq!(
        flex("font-weight: bold; font-stretch: condensed").unwrap(),
        pdf
    );
    // Explicit settings override the weight; the slant axis clamps at its -10 end.
    let pdf =
        flex("font-weight: bold; font-variation-settings: 'wght' 350; font-style: oblique 20deg")
            .unwrap();
    assert_eq!(
        base_fonts(&pdf),
        ["RobotoFlex-Regular", "RobotoFlex-Regular_350wght_-10slnt"]
    );
}

#[test]
fn given_an_axis_the_selected_font_lacks_when_compiling_then_rejects_naming_the_setting_and_the_fonts_axes(
) {
    assert_eq!(
        flex("font-variation-settings: \"GRAD\" 50").err().unwrap(),
        "fontVariationSettings: \"GRAD\" 50 names no axis of the selected font (its axes: wght, wdth, slnt) on <p> in <main>"
    );
    assert_eq!(
        flex("font-family: serif; font-style: oblique -10deg").err().unwrap(),
        "fontStyle: oblique -10deg needs a font with a slnt axis; without one, use oblique, which slants by 14deg on <p> in <main>"
    );
}
