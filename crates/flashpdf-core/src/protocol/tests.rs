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
    let runs: Vec<LinkedRun<'_>> = runs
        .iter()
        .map(|(font, size, color, flags, value)| (*font, *size, *color, *flags, None, *value))
        .collect();
    linked_paragraph(align, &[], &runs)
}

/// `(font, size, colour, flags, link index, text)`; a link index sets flag bit 3.
type LinkedRun<'a> = (u8, f32, [u8; 3], u8, Option<u16>, &'a [u8]);

/// `links` is the URI table the runs' link indices point into.
fn linked_paragraph(align: u8, links: &[&[u8]], runs: &[LinkedRun<'_>]) -> Vec<u8> {
    let mut payload = vec![align];
    payload.extend((links.len() as u16).to_le_bytes());
    for link in links {
        payload.extend(text(link));
    }
    payload.extend((runs.len() as u16).to_le_bytes());
    for (font, size, color, flags, link, value) in runs {
        payload.push(*font);
        payload.extend(size.to_le_bytes());
        payload.extend(color);
        payload.push(*flags | if link.is_some() { 8 } else { 0 });
        if let Some(link) = link {
            payload.extend(link.to_le_bytes());
        }
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
    bytes.extend(linked_paragraph(
        1,
        &[b"https://example.com"],
        &[(1, 10.0, [0xcc, 0, 0], 2, Some(0), b"Styled")],
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
        assert_eq!(
            font.get(b"Subtype").unwrap().as_name().unwrap(),
            b"TrueType"
        );
        assert_eq!(
            font.get(b"BaseFont").unwrap().as_name().unwrap(),
            b"Abel-Regular"
        );
        let descriptor = font.get(b"FontDescriptor").unwrap().as_reference().unwrap();
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
        assert_eq!(
            file.dict.get(b"Length1").unwrap().as_i64().unwrap(),
            font_bytes.len() as i64
        );
        assert_eq!(
            miniz_oxide::inflate::decompress_to_vec_zlib(&file.content).unwrap(),
            font_bytes
        );
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

    for control in ["\u{0b}", "\u{1e}", "\u{1f}"] {
        let mut sentinel = header(VERSION, 120.0, 60.0, 10.0);
        sentinel.extend(plain(control.as_bytes()));
        assert_eq!(push_error(&sentinel), "unsupported WinAnsi character");
    }

    let mut unsupported = header(VERSION, 120.0, 60.0, 10.0);
    unsupported.extend(plain("🙂".as_bytes()));
    assert_eq!(push_error(&unsupported), "unsupported WinAnsi character");

    let mut bad_length = header(VERSION, 120.0, 60.0, 10.0);
    let mut payload = vec![0, 0, 0, 1, 0, 0];
    payload.extend(10.0_f32.to_le_bytes());
    payload.extend([0, 0, 0, 0]);
    payload.extend(4_u32.to_le_bytes());
    payload.push(b'x');
    bad_length.extend(record(9, &payload));
    assert_eq!(push_error(&bad_length), "truncated record");

    let mut bad_flags = header(VERSION, 120.0, 60.0, 10.0);
    bad_flags.extend(paragraph(0, &[(0, 10.0, [0, 0, 0], 16, b"x")]));
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
    for value in [width, height] {
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
