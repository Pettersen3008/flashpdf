use super::{Decoder, HEADER_LEN, MAGIC, VERSION};

fn header(version: u16, width: f32, height: f32, margin: f32) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.extend(version.to_le_bytes());
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    bytes.extend(margin.to_le_bytes());
    assert_eq!(bytes.len(), HEADER_LEN);
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
    let mut bytes = (values.len() as u16).to_le_bytes().to_vec();
    for (kind, value) in values {
        bytes.push(*kind);
        bytes.extend(value.to_le_bytes());
    }
    bytes
}

/// `(font, size, colour, flags, text)`.
type Run<'a> = (u8, f32, [u8; 3], u8, &'a [u8]);

fn paragraph(align: u8, runs: &[Run<'_>]) -> Vec<u8> {
    let mut payload = vec![align];
    payload.extend((runs.len() as u16).to_le_bytes());
    for (font, size, color, flags, value) in runs {
        payload.push(*font);
        payload.extend(size.to_le_bytes());
        payload.extend(color);
        payload.push(*flags);
        payload.extend(text(value));
    }
    record(9, &payload)
}

/// `header_rows` then a full-width fraction track.
fn table_start(header_rows: u16) -> Vec<u8> {
    let mut bytes = header_rows.to_le_bytes().to_vec();
    bytes.push(1);
    bytes.extend(1.0_f32.to_le_bytes());
    bytes
}

fn plain(value: &[u8]) -> Vec<u8> {
    paragraph(0, &[(0, 10.0, [0, 0, 0], 0, value)])
}

fn box_start() -> Vec<u8> {
    let mut payload = Vec::new();
    for _ in 0..12 {
        payload.extend(0.0_f32.to_le_bytes());
    }
    payload.extend([1, 0xee, 0xee, 0xee]);
    payload.extend([0, 0, 0].repeat(4));
    record(17, &payload)
}

/// Every opcode the JSX lowering emits, on a 120x60 page whose 40pt of body
/// forces the stream across four pages.
fn representative() -> Vec<u8> {
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    bytes.extend(plain(b"Before"));
    bytes.extend(box_start());
    bytes.extend(plain(b"Boxed"));
    bytes.extend(record(18, &[]));
    bytes.extend(record(5, &columns(&[(0, 50.0), (1, 1.0)])));
    bytes.extend(plain(b"row"));
    bytes.extend(plain(b"1"));
    bytes.extend(record(6, &[]));
    bytes.extend(record(10, &table_start(1)));
    for cell in [b"head", b"body"] {
        bytes.extend(record(5, &columns(&[(1, 1.0)])));
        bytes.extend(plain(cell));
        bytes.extend(record(6, &[]));
    }
    bytes.extend(record(11, &[]));
    bytes.extend(record(2, &5.0_f32.to_le_bytes()));
    bytes.extend(record(3, &0.0_f32.to_le_bytes()));
    bytes.extend(plain(b"Stacked"));
    bytes.extend(record(4, &[]));
    bytes.extend(paragraph(1, &[(1, 10.0, [0xcc, 0, 0], 0, b"Styled")]));
    bytes.extend(record(7, &[]));
    bytes.extend(plain(b"After"));
    bytes.extend(record(255, &[]));
    bytes
}

fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = Decoder::default();
    decoder.push(bytes).map_err(|error| error.to_string())?;
    decoder.finish().map_err(|error| error.to_string())
}

#[test]
fn given_a_footer_when_decoding_then_repeats_resolved_page_numbers() {
    let mut bytes = header(VERSION, 595.0, 842.0, 36.0);
    let mut footer = 10.0_f32.to_le_bytes().to_vec();
    footer.extend([0, 0, 0, 0, 0]);
    footer.extend(text("Page \u{1e} of \u{1f}".as_bytes()));
    bytes.extend(record(8, &footer));
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
    assert_eq!(pdf.extract_text(&[1]).unwrap().trim(), "Hello World\nagain");
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
    assert_eq!(decoder.add_font(font_bytes.to_vec()).unwrap(), 2);
    assert_eq!(decoder.add_font(font_bytes.to_vec()).unwrap(), 3);
    let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
    for slot in [2, 3] {
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
    assert_eq!(pdf.get_pages().len(), 4);
    assert_eq!(
        pdf.extract_text(&[1]).unwrap().trim(),
        "Before\nBoxed\nrow\n1"
    );
    assert_eq!(
        pdf.extract_text(&[2]).unwrap().trim(),
        "head\nbody\nStacked"
    );
    assert_eq!(pdf.extract_text(&[3]).unwrap().trim(), "Styled");
    assert_eq!(pdf.extract_text(&[4]).unwrap().trim(), "After");
    for split in 0..=bytes.len() {
        let mut decoder = Decoder::default();
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

    let mut bad_version = representative();
    bad_version[4..6].copy_from_slice(&1_u16.to_le_bytes());
    assert_eq!(push_error(&bad_version), "unknown version");

    let mut unknown = header(VERSION, 120.0, 60.0, 10.0);
    unknown.extend(record(42, &[]));
    assert_eq!(push_error(&unknown), "unknown opcode");

    for legacy in [1, 16, 19] {
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
    let mut payload = vec![0, 1, 0, 0];
    payload.extend(10.0_f32.to_le_bytes());
    payload.extend([0, 0, 0, 0]);
    payload.extend(4_u32.to_le_bytes());
    payload.push(b'x');
    bad_length.extend(record(9, &payload));
    assert_eq!(push_error(&bad_length), "truncated record");

    let mut bad_flags = header(VERSION, 120.0, 60.0, 10.0);
    bad_flags.extend(paragraph(0, &[(0, 10.0, [0, 0, 0], 2, b"x")]));
    assert_eq!(push_error(&bad_flags), "invalid run flags");

    let mut bad_align = header(VERSION, 120.0, 60.0, 10.0);
    bad_align.extend(paragraph(3, &[(0, 10.0, [0, 0, 0], 0, b"x")]));
    assert_eq!(push_error(&bad_align), "invalid text alignment");

    let mut unknown_font = header(VERSION, 120.0, 60.0, 10.0);
    unknown_font.extend(paragraph(0, &[(2, 10.0, [0, 0, 0], 0, b"x")]));
    assert_eq!(push_error(&unknown_font), "unknown font");
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
fn given_mutated_protocol_streams_when_decoding_then_never_panics() {
    let original = representative();
    let mut rng = crate::tests::XorShift(0x2545_f491_4f6c_dd1d);
    for _ in 0..3000 {
        let mut bytes = original.clone();
        rng.mutate(&mut bytes);
        let mut decoder = Decoder::default();
        if decoder.push(&bytes).is_ok() {
            let _ = decoder.finish();
        }
    }
}
