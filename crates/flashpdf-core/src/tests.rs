use super::{
    Command, Edges, Fraction as FractionValue, Page, Percent as PercentValue, Pt, RenderError, Rgb,
    TextAlign, TextRun, TextStyle,
};

#[test]
fn given_valid_commands_when_rendered_then_pdf_is_valid_and_deterministic() {
    assert_eq!(Pt::new(f32::NAN), Err(RenderError::InvalidPoint));
    assert_eq!(Pt::new(-1.0), Err(RenderError::InvalidPoint));
    assert_eq!(
        Page::new(
            Pt::new(72.0).unwrap(),
            Pt::new(72.0).unwrap(),
            Pt::new(36.0).unwrap()
        ),
        Err(RenderError::InvalidPage)
    );
    let narrow_page = Page::new(
        Pt::new(72.0).unwrap(),
        Pt::new(100.0).unwrap(),
        Pt::new(10.0).unwrap(),
    )
    .unwrap();
    assert_eq!(
        super::render(
            narrow_page,
            &[Command::Text {
                text: "W",
                style: TextStyle::plain(Pt::new(60.0).unwrap()),
            }]
        ),
        Err(RenderError::TextTooWide)
    );
    assert_eq!(
        super::render(
            Page::A4,
            &[Command::Text {
                text: "\u{1f642}",
                style: TextStyle::plain(Pt::new(12.0).unwrap()),
            }]
        ),
        Err(RenderError::UnsupportedCharacter('\u{1f642}'))
    );

    let commands = [
        Command::Text {
            text: "First \u{20ac}",
            style: TextStyle::plain(Pt::new(14.0).unwrap()),
        },
        Command::Spacer(Pt::new(12.0).unwrap()),
        Command::Text {
            text: "After spacer",
            style: TextStyle::plain(Pt::new(10.0).unwrap()),
        },
        Command::PageBreak,
        Command::Text {
            text: "Second page",
            style: TextStyle::plain(Pt::new(12.0).unwrap()),
        },
    ];
    let bytes = super::render(Page::A4, &commands).unwrap();
    assert_eq!(bytes, super::render(Page::A4, &commands).unwrap());

    let pdf = lopdf::Document::load_mem(&bytes).unwrap();
    let pages = pdf.get_pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[&1], (4, 0));
    assert_eq!(pages[&2], (6, 0));
    assert_eq!(page_text(&pdf, 1), "First \u{20ac}\nAfter spacer");
    assert_eq!(page_text(&pdf, 2), "Second page");
    assert!(String::from_utf8(pdf.get_page_content((4, 0)))
        .unwrap()
        .contains("36 795.948 Td"));
    let fonts = pdf.get_page_fonts((4, 0)).unwrap();
    assert_eq!(fonts.len(), 1);
    assert_eq!(
        fonts[b"F1".as_slice()]
            .get(b"BaseFont")
            .unwrap()
            .as_name()
            .unwrap(),
        b"Helvetica"
    );
}

#[test]
fn given_a_content_stream_when_inflated_then_yields_the_painted_operators() {
    let bytes = super::render(Page::A4, &[text("hello")]).unwrap();
    let pdf = lopdf::Document::load_mem(&bytes).unwrap();
    let stream = pdf.get_object((5, 0)).unwrap().as_stream().unwrap();
    assert_eq!(
        stream.dict.get(b"Filter").unwrap().as_name().unwrap(),
        b"FlateDecode"
    );
    let inflated = miniz_oxide::inflate::decompress_to_vec_zlib(&stream.content).unwrap();
    assert_eq!(inflated, stream.decompressed_content().unwrap());
    assert_eq!(
        String::from_utf8(inflated).unwrap(),
        "BT\n0 0 0 rg\n/F1 10 Tf\n36 798.82 Td\n(hello) Tj\nET"
    );
}

#[test]
fn given_content_beyond_the_cap_when_pushing_then_rejects() {
    let mut renderer = super::Renderer::new(Page::A4);
    let long = Command::Text {
        text: &"W ".repeat(30_000),
        style: TextStyle::plain(Pt(100.0)),
    };
    let error = (0..1000)
        .find_map(|_| renderer.push(&[long]).err())
        .expect("content must stop growing");
    assert_eq!(error, RenderError::DocumentTooLarge);
}

/// xorshift64: enough randomness for mutation tests without a dependency.
pub(crate) struct XorShift(pub(crate) u64);

impl XorShift {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// One to three random flips, truncations or insertions.
    pub(crate) fn mutate(&mut self, bytes: &mut Vec<u8>) {
        for _ in 0..1 + self.next() % 3 {
            let position = (self.next() as usize) % (bytes.len() + 1);
            match self.next() % 3 {
                0 if !bytes.is_empty() => {
                    let index = position.min(bytes.len() - 1);
                    bytes[index] ^= 1 << (self.next() % 8);
                }
                1 => bytes.truncate(position),
                _ => bytes.insert(position, self.next() as u8),
            }
        }
    }
}

#[test]
fn given_mutated_fonts_when_parsing_and_rendering_then_never_panics() {
    let original = include_bytes!("../../../packages/flashpdf/test/fixtures/Abel-Regular.ttf");
    let mut rng = XorShift(0x9e37_79b9_7f4a_7c15);
    for round in 0..3000 {
        let mut bytes = original.to_vec();
        rng.mutate(&mut bytes);
        let Ok(mut font) = super::EmbeddedFont::parse(bytes) else {
            continue;
        };
        // Subsetting walks glyf/loca, which parsing does not validate; every round covers it.
        font.used.extend([1, 7, 40, 200]);
        let _ = font.program();
        // Deflating the font program dominates debug time; one in ten covers pdf.rs.
        if round % 10 != 0 {
            continue;
        }
        let mut renderer = super::Renderer::with_fonts(Page::A4, vec![font]);
        let style = TextStyle {
            font: super::FontId::new(12),
            ..TextStyle::plain(Pt(12.0))
        };
        let _ = renderer.push(&[Command::Text {
            text: "Fuzzed \u{20ac} text",
            style,
        }]);
        let _ = renderer.finish();
    }
}

fn page() -> Page {
    Page::new(Pt(120.0), Pt(60.0), Pt(10.0)).unwrap()
}

fn text(text: &str) -> Command<'_> {
    Command::Text {
        text,
        style: TextStyle::plain(Pt(10.0)),
    }
}

/// A page's text, without the space lopdf's extractor adds after each `TJ` array.
pub(crate) fn page_text(pdf: &lopdf::Document, page: u32) -> String {
    let text = pdf.extract_text(&[page]).unwrap();
    let lines: Vec<&str> = text.trim().lines().map(str::trim_end).collect();
    lines.join("\n")
}

fn parsed(commands: &[Command<'_>]) -> lopdf::Document {
    lopdf::Document::load_mem(&super::render(page(), commands).unwrap()).unwrap()
}

#[test]
fn given_exact_bottom_text_when_placed_then_stays_on_page() {
    let pdf = parsed(&[Command::Spacer(Pt(30.0)), text("bottom")]);
    assert_eq!(pdf.get_pages().len(), 1);
    assert!(String::from_utf8(pdf.get_page_content((4, 0)))
        .unwrap()
        .contains("10 12.82 Td"));
}

#[test]
fn given_exact_bottom_stack_when_placed_then_stays_on_page() {
    let pdf = parsed(&[
        Command::StackStart { gap: Pt(20.0) },
        text("a"),
        text("b"),
        Command::StackEnd,
    ]);
    assert_eq!(pdf.get_pages().len(), 1);
}

#[test]
fn given_exact_bottom_row_when_placed_then_stays_on_page() {
    let columns = [super::ColumnWidth::Fixed(Pt(100.0))];
    let pdf = parsed(&[
        Command::Spacer(Pt(20.0)),
        Command::RowStart { columns: &columns },
        Command::StackStart { gap: Pt(0.0) },
        text("a"),
        text("b"),
        Command::StackEnd,
        Command::RowEnd,
    ]);
    assert_eq!(pdf.get_pages().len(), 1);
}

#[test]
fn given_overflow_when_placing_block_then_moves_whole_block_to_next_page() {
    let pdf = parsed(&[
        text("first"),
        Command::StackStart { gap: Pt(20.0) },
        text("a"),
        text("b"),
        Command::StackEnd,
    ]);
    assert_eq!(pdf.get_pages().len(), 2);
    assert_eq!(page_text(&pdf, 1), "first");
    assert_eq!(page_text(&pdf, 2), "a\nb");
}

#[test]
fn given_empty_text_when_at_bottom_then_consumes_no_height() {
    let pdf = parsed(&[Command::Spacer(Pt(40.0)), text(""), text(" \t\n")]);
    assert_eq!(pdf.get_pages().len(), 1);
    assert!(pdf.get_page_content((4, 0)).is_empty());
}

/// Line starts only; a later segment on the same line moves with `dx 0 Td`.
fn baselines(pdf: &lopdf::Document, page: lopdf::ObjectId) -> Vec<f32> {
    String::from_utf8(pdf.get_page_content(page))
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_suffix(" Td"))
        .map(|line| line.split(' ').nth(1).unwrap().parse().unwrap())
        .filter(|y| *y != 0.0)
        .collect()
}

#[test]
fn given_a_three_hundred_line_paragraph_when_rendered_then_streams_across_pages_without_overlap() {
    // Each 64pt word fills a 100pt line alone.
    let words = (1..=300)
        .map(|n| format!("WWWWW{n:03}"))
        .collect::<Vec<_>>();
    let paragraph = words.join(" ");
    let pdf = parsed(&[text("first"), text(&paragraph), text("last")]);
    // 40pt of body holds four 9.25pt lines; 301 lines then need 76 pages.
    let pages = pdf.get_pages();
    assert_eq!(pages.len(), 76);
    let mut total = 0;
    for id in pages.values() {
        let ys = baselines(&pdf, *id);
        total += ys.len();
        assert!(ys.windows(2).all(|pair| pair[0] - pair[1] > 9.2));
        assert!(ys.iter().all(|y| *y > 12.0));
    }
    assert_eq!(total, 302);
    assert_eq!(page_text(&pdf, 1), "first\nWWWWW001\nWWWWW002\nWWWWW003");
    assert!(page_text(&pdf, 76).ends_with("last"));
}

#[test]
fn given_a_word_wider_than_the_column_when_wrapping_then_breaks_it_at_the_last_fitting_glyph() {
    // "i" is 2.22pt, "W" 9.44pt; the run fits 100pt only when split by glyph.
    let pdf = parsed(&[text(&format!("ab {}WW cd", "i".repeat(50)))]);
    assert_eq!(
        page_text(&pdf, 1),
        format!("ab\n{}\n{}WW cd", "i".repeat(45), "i".repeat(5))
    );
    let pdf = parsed(&[text(&"W".repeat(25))]);
    assert_eq!(
        page_text(&pdf, 1),
        format!("{}\n{}\n{}", "W".repeat(10), "W".repeat(10), "W".repeat(5))
    );
}

fn run(len: usize, bold: bool, size: f32, hard_break: bool) -> TextRun {
    TextRun {
        len,
        font: if bold {
            super::HELVETICA_BOLD
        } else {
            super::HELVETICA
        },
        size: Pt(size),
        line_height: None,
        color: Rgb::BLACK,
        hard_break,
        decoration: Default::default(),
        typography: super::Typography::NORMAL,
    }
}

fn content(pdf: &lopdf::Document) -> String {
    let page = *pdf.get_pages().values().next().unwrap();
    String::from_utf8(pdf.get_page_content(page)).unwrap()
}

#[test]
fn given_a_word_spanning_two_runs_when_wrapping_then_moves_it_whole_and_paints_both_fonts_in_one_text_object(
) {
    // "WWWWW" fits the 100pt line; "WWWW"+"WW" is one 56.64pt word that does not follow it.
    let runs = [run(10, false, 10.0, false), run(2, true, 10.0, false)];
    let pdf = parsed(&[Command::Paragraph {
        links: &[],
        align: TextAlign::Left,
        text: "WWWWW WWWWWW",
        runs: &runs,
    }]);
    assert_eq!(page_text(&pdf, 1), "WWWWW\nWWWWWW");
    let content = content(&pdf);
    let objects: Vec<&str> = content.split("ET").collect();
    assert_eq!(objects.len(), 3, "{content}");
    assert_eq!(
        objects[1].trim(),
        "BT\n0 0 0 rg\n/F1 10 Tf\n10 33.57 Td\n(WWWW) Tj\n/F2 10 Tf\n37.76 0 Td\n(WW) Tj"
    );
}

#[test]
fn given_a_hard_break_when_laying_out_then_starts_a_new_line_without_a_space() {
    let runs = [run(2, false, 10.0, true), run(1, false, 10.0, false)];
    let pdf = parsed(&[Command::Paragraph {
        links: &[],
        align: TextAlign::Left,
        text: "a b",
        runs: &runs,
    }]);
    assert_eq!(page_text(&pdf, 1), "a\nb");
    assert_eq!(baselines(&pdf, (4, 0)), [42.82, 33.57]);
}

#[test]
fn given_mixed_sizes_on_one_line_when_laying_out_then_the_line_takes_the_larger_metrics() {
    let runs = [run(1, false, 10.0, false), run(1, false, 20.0, false)];
    let pdf = parsed(&[
        Command::Paragraph {
            links: &[],
            align: TextAlign::Left,
            text: "ab",
            runs: &runs,
        },
        text("c"),
    ]);
    // Baseline sits at the 20pt ascent (14.36); the next block starts after the 20pt line height (18.5).
    assert_eq!(baselines(&pdf, (4, 0)), [35.64, 24.32]);
    assert!(content(&pdf).contains("/F1 20 Tf\n5.56 0 Td\n(b) Tj"));
}

#[test]
fn given_runs_that_do_not_cover_the_text_when_laying_out_then_rejects() {
    let runs = [run(1, false, 10.0, false)];
    assert_eq!(
        super::render(
            page(),
            &[Command::Paragraph {
                links: &[],
                align: TextAlign::Left,
                text: "ab",
                runs: &runs,
            }]
        ),
        Err(RenderError::InvalidLayout)
    );
}

#[test]
fn given_a_single_line_taller_than_the_page_when_streaming_then_rejects() {
    assert_eq!(
        super::render(
            page(),
            &[Command::Text {
                text: "x",
                style: TextStyle::plain(Pt(50.0)),
            }]
        ),
        Err(RenderError::PageOverflow)
    );
}

#[test]
fn given_a_bordered_box_taller_than_the_page_when_placed_then_rejects() {
    let style = super::BoxStyle {
        margin: Edges::all(Pt(0.0)),
        padding: Edges::all(Pt(0.0)),
        border: Edges::all(Pt(1.0)),
        background: None,
        border_color: Edges::all(Rgb::BLACK),
    };
    let long = "word ".repeat(40);
    assert_eq!(
        super::render(
            page(),
            &[Command::BoxStart { style }, text(&long), Command::BoxEnd]
        ),
        Err(RenderError::PageOverflow)
    );
    let plain = super::BoxStyle {
        border: Edges::all(Pt(0.0)),
        margin: Edges::all(Pt(2.0)),
        ..style
    };
    let pdf = parsed(&[
        Command::BoxStart { style: plain },
        text(&long),
        Command::BoxEnd,
    ]);
    assert_eq!(pdf.get_pages().len(), 3);
}

#[test]
fn given_oversized_block_when_measured_then_rejects() {
    assert_eq!(
        super::render(
            page(),
            &[
                Command::StackStart { gap: Pt(22.0) },
                text("a"),
                text("b"),
                Command::StackEnd
            ]
        ),
        Err(RenderError::PageOverflow)
    );
    assert_eq!(
        super::render(page(), &[Command::Spacer(Pt(41.0))]),
        Err(RenderError::PageOverflow)
    );
}

#[test]
fn given_columns_when_wrapped_then_pdf_has_measured_positions_and_deterministic_bytes() {
    use super::ColumnWidth::{Fixed, Fraction};
    let columns = [
        Fixed(Pt(20.0)),
        Fraction(FractionValue::new(1.0).unwrap()),
        Fraction(FractionValue::new(3.0).unwrap()),
    ];
    let commands = [
        Command::RowStart { columns: &columns },
        Command::StackStart { gap: Pt(0.0) },
        text("aa \t aa"),
        Command::StackEnd,
        text("bb bb"),
        text("last"),
        Command::RowEnd,
        text("after"),
    ];
    let bytes = super::render(page(), &commands).unwrap();
    assert_eq!(bytes, super::render(page(), &commands).unwrap());
    let pdf = lopdf::Document::load_mem(&bytes).unwrap();
    assert_eq!(pdf.get_pages().len(), 1);
    assert_eq!(page_text(&pdf, 1), "aa\naa\nbb\nbb\nlast\nafter");
    let content = String::from_utf8(pdf.get_page_content((4, 0))).unwrap();
    for position in [
        "10 42.82 Td",
        "10 33.57 Td",
        "30 42.82 Td",
        "30 33.57 Td",
        "50 42.82 Td",
        "10 24.32 Td",
    ] {
        assert!(content.contains(position), "{position}: {content}");
    }
}

#[test]
fn given_percent_and_fraction_columns_when_measured_then_percent_uses_row_width() {
    use super::ColumnWidth::{Fraction, Percent};
    let columns = [
        Percent(PercentValue::new(50.0).unwrap()),
        Fraction(FractionValue::new(1.0).unwrap()),
    ];
    let pdf = parsed(&[
        Command::RowStart { columns: &columns },
        text("left"),
        text("right"),
        Command::RowEnd,
    ]);
    let content = String::from_utf8(pdf.get_page_content((4, 0))).unwrap();
    assert!(content.contains("10 42.82 Td"));
    assert!(content.contains("60 42.82 Td"));
}

#[test]
fn given_percent_columns_totalling_one_hundred_when_rounded_then_accepts_them() {
    use super::ColumnWidth::Percent;
    let columns = [
        Percent(PercentValue::new(0.1).unwrap()),
        Percent(PercentValue::new(99.9).unwrap()),
    ];
    assert_eq!(
        parsed(&[
            Command::RowStart { columns: &columns },
            text(""),
            text(""),
            Command::RowEnd,
        ])
        .get_pages()
        .len(),
        1
    );
}

fn table_start(header_rows: u16) -> Command<'static> {
    Command::TableStart {
        header_rows,
        footer_rows: 0,
        width: super::ColumnWidth::Fraction(FractionValue::new(1.0).unwrap()),
    }
}

#[test]
fn given_a_table_with_two_header_rows_and_sixty_body_rows_when_rendered_then_repeats_the_header_per_page_and_paints_each_row_once(
) {
    let columns = [super::ColumnWidth::Fraction(FractionValue::new(1.0).unwrap()); 3];
    let cells: Vec<String> = (1..=60)
        .flat_map(|row| ["a", "b", "c"].map(|column| format!("r{row}{column}")))
        .collect();
    let mut commands = vec![table_start(2)];
    for header in [["H1a", "H1b", "H1c"], ["H2a", "H2b", "H2c"]] {
        commands.push(Command::RowStart { columns: &columns });
        commands.extend(header.iter().map(|cell| text(cell)));
        commands.push(Command::RowEnd);
    }
    for row in cells.chunks(3) {
        commands.push(Command::RowStart { columns: &columns });
        commands.extend(row.iter().map(|cell| text(cell)));
        commands.push(Command::RowEnd);
    }
    commands.push(Command::TableEnd);
    let pdf = parsed(&commands);
    // 40pt of body holds the 18.5pt header plus two 9.25pt rows.
    assert_eq!(pdf.get_pages().len(), 30);
    let mut body = Vec::new();
    for page in 1..=30 {
        let page_text = page_text(&pdf, page);
        let rest = page_text
            .strip_prefix("H1a\nH1b\nH1c\nH2a\nH2b\nH2c\n")
            .unwrap_or_else(|| panic!("page {page} lacks the header: {page_text}"));
        body.extend(rest.lines().map(str::to_owned));
    }
    assert_eq!(body, cells);
}

#[test]
fn given_a_painted_cell_beside_a_taller_cell_when_laid_out_then_its_background_covers_the_row_height(
) {
    use super::ColumnWidth::{Fixed, Fraction};
    let style = super::BoxStyle {
        margin: Edges::all(Pt(0.0)),
        padding: Edges::all(Pt(0.0)),
        border: Edges::all(Pt(0.0)),
        background: Some(Rgb { r: 255, g: 0, b: 0 }),
        border_color: Edges::all(Rgb::BLACK),
    };
    let columns = [Fixed(Pt(50.0)), Fraction(FractionValue::new(1.0).unwrap())];
    let pdf = parsed(&[
        Command::RowStart { columns: &columns },
        Command::BoxStart { style },
        text("a"),
        Command::BoxEnd,
        Command::StackStart { gap: Pt(0.0) },
        text("a"),
        text("b"),
        Command::StackEnd,
        Command::RowEnd,
    ]);
    assert!(
        content(&pdf).contains("10 31.5 50 18.5 re"),
        "{}",
        content(&pdf)
    );
}

#[test]
fn given_a_table_row_taller_than_the_page_with_its_header_when_rendered_then_rejects_naming_the_row(
) {
    let columns = [super::ColumnWidth::Fraction(
        FractionValue::new(1.0).unwrap(),
    )];
    let mut commands = vec![
        table_start(1),
        Command::RowStart { columns: &columns },
        text("header"),
        Command::RowEnd,
        Command::RowStart { columns: &columns },
        Command::StackStart { gap: Pt(0.0) },
    ];
    commands.extend(std::iter::repeat_n(text("line"), 4));
    commands.extend([Command::StackEnd, Command::RowEnd, Command::TableEnd]);
    assert_eq!(
        super::render(page(), &commands),
        Err(RenderError::TableRowOverflow(1))
    );
    // Three lines under a one-line header fit an empty page exactly, so the row moves whole.
    commands.remove(7);
    let pdf = parsed(&commands);
    assert_eq!(pdf.get_pages().len(), 1);
    assert_eq!(page_text(&pdf, 1), "header\nline\nline\nline");
}

#[test]
fn given_nested_backgrounds_when_rendered_then_parent_paint_precedes_child_paint() {
    let red = super::BoxStyle {
        margin: Edges::all(Pt(0.0)),
        padding: Edges::all(Pt(1.0)),
        border: Edges::all(Pt(0.0)),
        background: Some(Rgb { r: 255, g: 0, b: 0 }),
        border_color: Edges::all(Rgb::BLACK),
    };
    let blue = super::BoxStyle {
        background: Some(Rgb { r: 0, g: 0, b: 255 }),
        ..red
    };
    let pdf = parsed(&[
        Command::BoxStart { style: red },
        Command::BoxStart { style: blue },
        text("x"),
        Command::BoxEnd,
        Command::BoxEnd,
    ]);
    let content = String::from_utf8(pdf.get_page_content((4, 0))).unwrap();
    assert!(content.find("1 0 0 rg").unwrap() < content.find("0 0 1 rg").unwrap());
}

#[test]
fn given_bold_helvetica_when_measuring_then_uses_bold_widths() {
    use super::font::Font;
    assert_ne!(
        Font::Standard(super::HELVETICA.slot()).advance(b"A"),
        Font::Standard(super::HELVETICA_BOLD.slot()).advance(b"A"),
    );
}

/// The renderer keeps one scratch buffer across blocks, so a long document
/// must not grow it. This is what lets a caller stream pages of content.
#[test]
fn given_one_hundred_thousand_blocks_when_pushed_then_scratch_stays_flat() {
    let mut renderer = super::Renderer::new(Page::A4);
    renderer.push(&[text("row 1")]).unwrap();
    let initial = renderer.scratch_capacity();
    for _ in 1..100_000 {
        renderer.push(&[text("row 1")]).unwrap();
    }
    assert_eq!(renderer.scratch_capacity(), initial);
    assert!(lopdf::Document::load_mem(&renderer.finish().unwrap()).is_ok());
}

#[test]
fn given_invalid_containers_when_measured_then_rejects() {
    use super::ColumnWidth::{Fixed, Fraction};
    for commands in [
        vec![Command::StackEnd],
        vec![Command::StackStart { gap: Pt(0.0) }],
        vec![
            Command::StackStart { gap: Pt(0.0) },
            Command::PageBreak,
            Command::StackEnd,
        ],
        vec![Command::RowStart { columns: &[] }, Command::RowEnd],
        vec![
            Command::RowStart {
                columns: &[Fixed(Pt(101.0))],
            },
            text("x"),
            Command::RowEnd,
        ],
        vec![
            Command::RowStart {
                columns: &[Fixed(Pt(20.0))],
            },
            Command::RowEnd,
        ],
        vec![
            Command::RowStart {
                columns: &[Fixed(Pt(20.0))],
            },
            text("x"),
            text("y"),
            Command::RowEnd,
        ],
        vec![Command::TableEnd],
        vec![table_start(1), Command::TableEnd],
        vec![table_start(0), text("x"), Command::TableEnd],
        vec![
            table_start(0),
            Command::RowStart {
                columns: &[Fixed(Pt(20.0))],
            },
            text("x"),
            Command::RowEnd,
            Command::RowStart {
                columns: &[Fixed(Pt(20.0)), Fixed(Pt(20.0))],
            },
            text("x"),
            text("y"),
            Command::RowEnd,
            Command::TableEnd,
        ],
    ] {
        assert_eq!(
            super::render(page(), &commands),
            Err(RenderError::InvalidLayout)
        );
    }
    assert_eq!(FractionValue::new(0.0), Err(RenderError::InvalidLayout));
    let too_many = [Fraction(FractionValue::new(1.0).unwrap()); 257];
    assert_eq!(
        super::render(page(), &[Command::RowStart { columns: &too_many }]),
        Err(RenderError::InvalidLayout)
    );
    let mut allowed = vec![Command::StackStart { gap: Pt(0.0) }; 64];
    allowed.push(text("x"));
    allowed.extend(vec![Command::StackEnd; 64]);
    assert!(super::render(page(), &allowed).is_ok());
    let mut deep = vec![Command::StackStart { gap: Pt(0.0) }; 65];
    deep.push(text("x"));
    deep.extend(vec![Command::StackEnd; 65]);
    assert_eq!(
        super::render(page(), &deep),
        Err(RenderError::InvalidLayout)
    );
}

const ABEL: &[u8] = include_bytes!("../../../packages/flashpdf/test/fixtures/Abel-Regular.ttf");

fn abel_text(text: &str) -> Result<Vec<u8>, RenderError> {
    let font = super::EmbeddedFont::parse(ABEL.to_vec()).unwrap();
    let mut renderer = super::Renderer::with_fonts(Page::A4, vec![font]);
    renderer.push(&[Command::Text {
        text,
        style: TextStyle {
            font: super::FontId::new(12),
            ..TextStyle::plain(Pt(12.0))
        },
    }])?;
    renderer.finish()
}

#[test]
fn given_abel_and_text_outside_winansi_when_rendered_then_emits_a_subset_type0_font_whose_text_round_trips(
) {
    let text = "Ærø – 3 € ł";
    let bytes = abel_text(text).unwrap();
    assert_eq!(bytes, abel_text(text).unwrap());
    let pdf = lopdf::Document::load_mem(&bytes).unwrap();
    assert_eq!(page_text(&pdf, 1), text);
    let page = *pdf.get_pages().values().next().unwrap();
    let font = &pdf.get_page_fonts(page).unwrap()[b"F5".as_slice()];
    assert_eq!(font.get(b"Subtype").unwrap().as_name().unwrap(), b"Type0");
    assert_eq!(
        font.get(b"Encoding").unwrap().as_name().unwrap(),
        b"Identity-H"
    );
    let base_font = font.get(b"BaseFont").unwrap().as_name().unwrap();
    assert!(
        base_font.len() == 19 && base_font.ends_with(b"+Abel-Regular"),
        "{}",
        String::from_utf8_lossy(base_font)
    );
    assert!(font.get(b"ToUnicode").unwrap().as_reference().is_ok());
    let descendant = font.get(b"DescendantFonts").unwrap().as_array().unwrap()[0]
        .as_reference()
        .unwrap();
    let descendant = pdf.get_dictionary(descendant).unwrap();
    assert_eq!(
        descendant.get(b"Subtype").unwrap().as_name().unwrap(),
        b"CIDFontType2"
    );
    assert_eq!(
        descendant.get(b"CIDToGIDMap").unwrap().as_name().unwrap(),
        b"Identity"
    );
    // Ten distinct glyphs: one `/W` group per run of consecutive ids, each a start and an array.
    let widths = descendant.get(b"W").unwrap().as_array().unwrap();
    assert!(
        widths.len().is_multiple_of(2) && widths.len() <= 20,
        "{widths:?}"
    );
    let descriptor = descendant
        .get(b"FontDescriptor")
        .unwrap()
        .as_reference()
        .unwrap();
    let file = pdf
        .get_dictionary(descriptor)
        .unwrap()
        .get(b"FontFile2")
        .unwrap()
        .as_reference()
        .unwrap();
    let file = pdf.get_object(file).unwrap().as_stream().unwrap();
    let program = file.decompressed_content().unwrap();
    assert_eq!(
        file.dict.get(b"Length1").unwrap().as_i64().unwrap(),
        program.len() as i64
    );
    assert!(program.len() < ABEL.len() / 3, "{}", program.len());
}

#[test]
fn given_a_character_abel_lacks_when_rendered_then_rejects_naming_it() {
    assert_eq!(
        abel_text("\u{4e2d}").unwrap_err(),
        RenderError::MissingGlyph('\u{4e2d}')
    );
}

#[test]
fn given_a_15pt_line_height_on_three_lines_when_rendered_then_the_third_line_moves_to_the_next_page(
) {
    // 40pt of content holds three normal 9.25pt lines, but only two 15pt ones.
    let runs = [true, true, false].map(|hard_break| TextRun {
        line_height: Some(Pt(15.0)),
        ..run(1, false, 10.0, hard_break)
    });
    let pdf = parsed(&[Command::Paragraph {
        links: &[],
        align: TextAlign::Left,
        text: "abc",
        runs: &runs,
    }]);
    assert_eq!(pdf.get_pages().len(), 2);
    assert_eq!(page_text(&pdf, 1), "a\nb");
    assert_eq!(page_text(&pdf, 2), "c");
}

#[test]
fn given_a_table_with_a_footer_row_when_it_spans_pages_then_the_footer_follows_the_last_body_row_on_every_page(
) {
    let columns = [super::ColumnWidth::Fraction(
        FractionValue::new(1.0).unwrap(),
    )];
    let cells: Vec<String> = (1..=6).map(|row| format!("r{row}")).collect();
    let mut commands = vec![Command::TableStart {
        header_rows: 1,
        footer_rows: 1,
        width: columns[0],
    }];
    for cell in std::iter::once("H")
        .chain(cells.iter().map(String::as_str))
        .chain(["F"])
    {
        commands.extend([
            Command::RowStart { columns: &columns },
            text(cell),
            Command::RowEnd,
        ]);
    }
    commands.push(Command::TableEnd);
    let pdf = parsed(&commands);
    // 40pt of body holds the 9.25pt header and footer plus two 9.25pt rows.
    assert_eq!(pdf.get_pages().len(), 3);
    for (page, rows) in (1..=3).zip(cells.chunks(2)) {
        let expected = format!("H\n{}\nF", rows.join("\n"));
        assert_eq!(page_text(&pdf, page), expected);
    }
}

#[test]
fn given_a_header_and_a_line_taller_than_the_page_when_rendered_then_rejects_with_page_overflow() {
    let (sender, receiver) = std::sync::mpsc::channel();
    // A thread, so a pagination loop that never ends fails the test instead of hanging it.
    std::thread::spawn(move || {
        let mut renderer = super::Renderer::new(page());
        renderer
            .set_header("Header".into(), TextStyle::plain(Pt(10.0)))
            .unwrap();
        let runs = [TextRun {
            line_height: Some(Pt(100.0)),
            ..run(1, false, 10.0, false)
        }];
        let _ = sender.send(renderer.push(&[Command::Paragraph {
            links: &[],
            align: TextAlign::Left,
            text: "a",
            runs: &runs,
        }]));
    });
    let result = receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("pagination must terminate");
    assert_eq!(result, Err(RenderError::PageOverflow));
}

#[test]
fn given_a_header_and_footer_when_a_stack_or_table_row_is_taller_than_the_body_between_them_then_rejects(
) {
    let render = |commands: &[Command<'_>]| {
        let mut renderer = super::Renderer::new(page());
        for set in [super::Renderer::set_header, super::Renderer::set_footer] {
            set(&mut renderer, "x".into(), TextStyle::plain(Pt(10.0))).unwrap();
        }
        renderer.push(commands)?;
        renderer.finish()
    };
    // 40pt of content minus the 9.25pt header and footer holds two 9.25pt lines, not three.
    let mut stack = vec![Command::StackStart { gap: Pt(0.0) }];
    stack.extend(std::iter::repeat_n(text("line"), 3));
    stack.push(Command::StackEnd);
    let columns = [super::ColumnWidth::Fraction(
        FractionValue::new(1.0).unwrap(),
    )];
    let mut table = vec![table_start(0), Command::RowStart { columns: &columns }];
    table.extend(stack.iter().copied());
    table.extend([Command::RowEnd, Command::TableEnd]);
    assert_eq!(render(&stack).err(), Some(RenderError::PageOverflow));
    assert_eq!(render(&table).err(), Some(RenderError::TableRowOverflow(0)));
    stack.remove(1);
    let pdf = lopdf::Document::load_mem(&render(&stack).unwrap()).unwrap();
    assert_eq!(pdf.get_pages().len(), 1);
}

#[test]
fn given_two_sizes_with_the_same_line_height_on_one_line_when_wrapping_then_the_line_box_spans_the_tallest_ascent_and_deepest_depth(
) {
    // In a 20pt line, 20pt text rises 15.11pt and 10pt text sinks 7.445pt below the baseline.
    let runs = [20.0, 10.0].map(|size| TextRun {
        line_height: Some(Pt(20.0)),
        ..run(1, false, size, false)
    });
    let pdf = parsed(&[
        Command::Paragraph {
            links: &[],
            align: TextAlign::Left,
            text: "ab",
            runs: &runs,
        },
        text("next"),
    ]);
    // The 22.555pt line box puts the next baseline at 50 - 22.555 - 7.18.
    let next = baselines(&pdf, (4, 0))[1];
    assert!((next - 20.265).abs() < 0.001, "{next}");
}

#[test]
fn given_a_long_footer_and_many_page_breaks_when_finishing_then_rejects_past_the_content_cap() {
    let mut renderer = super::Renderer::new(Page::A4);
    // 60,000 glyphs at 0.01pt fit the line, so every page paints about 60 KB of footer.
    renderer
        .set_footer("x".repeat(60_000), TextStyle::plain(Pt(0.01)))
        .unwrap();
    renderer.push(&vec![Command::PageBreak; 300]).unwrap();
    assert_eq!(renderer.finish().err(), Some(RenderError::DocumentTooLarge));
}

fn operations(pdf: &lopdf::Document) -> Vec<lopdf::content::Operation> {
    let page = *pdf.get_pages().values().next().unwrap();
    pdf.get_and_decode_page_content(page).unwrap().operations
}

/// Numeric operands of every `name` operation, or of its array operand for `TJ`.
fn operands(pdf: &lopdf::Document, name: &str) -> Vec<Vec<f32>> {
    operations(pdf)
        .iter()
        .filter(|operation| operation.operator == name)
        .map(|operation| {
            let items = match operation.operands.first() {
                Some(lopdf::Object::Array(items)) => items.as_slice(),
                _ => operation.operands.as_slice(),
            };
            items
                .iter()
                .filter_map(|item| item.as_float().ok())
                .collect()
        })
        .collect()
}

#[test]
fn given_justified_text_that_wraps_when_rendered_then_only_the_wrapped_line_stretches_its_spaces() {
    let runs = [run(24, false, 10.0, false)];
    let pdf = parsed(&[Command::Paragraph {
        links: &[],
        align: TextAlign::Justify,
        text: "aaaa aaaa aaaa aaaa aaaa",
        runs: &runs,
    }]);
    // Four words take 97.3pt of 100, so each of the three spaces gains 0.9pt: 90/1000 em at 10pt.
    let stretched = operands(&pdf, "TJ");
    assert_eq!(stretched.len(), 1);
    assert_eq!(stretched[0].len(), 3);
    assert!(stretched[0].iter().all(|gap| (gap + 90.0).abs() < 0.01));
    assert_eq!(operands(&pdf, "Tj").len(), 1);
}

#[test]
fn given_letter_spacing_when_wrapping_then_measures_it_after_every_glyph_and_paints_it_as_tc() {
    let style = TextStyle {
        typography: super::Typography {
            letter_spacing: 1.0,
            ..super::Typography::NORMAL
        },
        ..TextStyle::plain(Pt(10.0))
    };
    // Ten 9.44pt W fit 100pt, but not at 10.44pt each.
    let pdf = parsed(&[Command::Text {
        text: "WWWWWWWWWW",
        style,
    }]);
    assert_eq!(page_text(&pdf, 1), "WWWWWWWWW\nW");
    assert_eq!(operands(&pdf, "Tc"), [[1.0], [0.0], [1.0], [0.0]]);
}

#[test]
fn given_word_spacing_in_an_embedded_font_when_rendered_then_widens_each_space_with_a_tj_adjustment(
) {
    let font = super::EmbeddedFont::parse(ABEL.to_vec()).unwrap();
    let mut renderer = super::Renderer::with_fonts(Page::A4, vec![font]);
    let style = TextStyle {
        font: super::FontId::new(12),
        typography: super::Typography {
            word_spacing: 3.0,
            ..super::Typography::NORMAL
        },
        ..TextStyle::plain(Pt(12.0))
    };
    renderer
        .push(&[Command::Text {
            text: "a b c",
            style,
        }])
        .unwrap();
    let pdf = lopdf::Document::load_mem(&renderer.finish().unwrap()).unwrap();
    // Tw skips two-byte codes, so 3pt at 12pt is a -250 adjustment after each space.
    assert_eq!(operands(&pdf, "TJ"), [[-250.0, -250.0]]);
    assert!(operands(&pdf, "Tw").is_empty());
}

#[test]
fn given_a_slanted_run_when_rendered_then_skews_its_text_matrix_by_a_quarter() {
    let style = TextStyle {
        typography: super::Typography {
            oblique: true,
            ..super::Typography::NORMAL
        },
        ..TextStyle::plain(Pt(10.0))
    };
    let pdf = parsed(&[Command::Text { text: "x", style }]);
    assert_eq!(operands(&pdf, "Tm"), [[1.0, 0.0, 0.25, 1.0, 10.0, 42.82]]);
    assert!(operands(&pdf, "Td").is_empty());
}

#[test]
fn given_the_oblique_helvetica_slots_when_rendered_then_names_the_standard_oblique_faces() {
    for (font, name) in [
        (super::HELVETICA_OBLIQUE, "Helvetica-Oblique"),
        (super::HELVETICA_BOLD_OBLIQUE, "Helvetica-BoldOblique"),
    ] {
        let pdf = parsed(&[Command::Text {
            text: "x",
            style: TextStyle {
                font,
                ..TextStyle::plain(Pt(10.0))
            },
        }]);
        let page = *pdf.get_pages().values().next().unwrap();
        let fonts = pdf.get_page_fonts(page).unwrap();
        let base_font = fonts.values().next().unwrap().get(b"BaseFont").unwrap();
        assert_eq!(base_font.as_name().unwrap(), name.as_bytes());
    }
}

#[test]
fn given_an_underlined_struck_run_with_a_decoration_colour_when_painted_then_strokes_use_helvetica_afm_metrics_in_that_colour(
) {
    let runs = [TextRun {
        decoration: super::RunDecoration {
            underline: true,
            line_through: true,
            color: Some(Rgb { r: 255, g: 0, b: 0 }),
            ..Default::default()
        },
        ..run(1, false, 10.0, false)
    }];
    let pdf = parsed(&[Command::Paragraph {
        links: &[],
        align: TextAlign::Left,
        text: "x",
        runs: &runs,
    }]);
    let baseline = operands(&pdf, "Td")[0][1];
    let strokes = operands(&pdf, "re");
    assert_eq!(strokes.len(), 2);
    // AFM UnderlinePosition -100 and thickness 50; the strike sits at half the 523 x-height.
    let expected = [(baseline - 1.25, 0.5), (baseline + 2.37, 0.5)];
    for (stroke, (y, height)) in strokes.iter().zip(expected) {
        assert!(
            (stroke[1] - y).abs() < 0.001 && stroke[3] == height,
            "{stroke:?}"
        );
    }
    assert_eq!(operands(&pdf, "rg").last().unwrap(), &[1.0, 0.0, 0.0]);
}

#[test]
fn given_ava_in_helvetica_when_laid_out_then_kerns_each_pair_by_its_afm_value_unless_kerning_is_off(
) {
    let render = |kerning| {
        let style = TextStyle {
            align: TextAlign::Right,
            typography: super::Typography {
                kerning,
                ..super::Typography::NORMAL
            },
            ..TextStyle::plain(Pt(10.0))
        };
        parsed(&[Command::Text { text: "AVA", style }])
    };
    let (kerned, plain) = (render(true), render(false));
    // KPX A V -70 and V A -80 tighten the right-aligned word by 1.5pt at 10pt.
    assert_eq!(operands(&kerned, "TJ"), [[70.0, 80.0]]);
    assert!(operands(&plain, "TJ").is_empty());
    let x = |pdf: &lopdf::Document| operands(pdf, "Td")[0][0];
    assert!((x(&kerned) - x(&plain) - 1.5).abs() < 0.001);
}

#[test]
fn given_the_times_and_courier_slots_when_rendered_then_names_the_standard_faces() {
    for (font, name, resource) in [
        (super::TIMES, "Times-Roman", "T1"),
        (super::TIMES_BOLD, "Times-Bold", "T2"),
        (super::TIMES_ITALIC, "Times-Italic", "T3"),
        (super::TIMES_BOLD_ITALIC, "Times-BoldItalic", "T4"),
        (super::COURIER, "Courier", "C1"),
        (super::COURIER_BOLD, "Courier-Bold", "C2"),
        (super::COURIER_OBLIQUE, "Courier-Oblique", "C3"),
        (super::COURIER_BOLD_OBLIQUE, "Courier-BoldOblique", "C4"),
    ] {
        let pdf = parsed(&[Command::Text {
            text: "x",
            style: TextStyle {
                font,
                ..TextStyle::plain(Pt(10.0))
            },
        }]);
        let page = *pdf.get_pages().values().next().unwrap();
        let fonts = pdf.get_page_fonts(page).unwrap();
        let base_font = fonts[resource.as_bytes()].get(b"BaseFont").unwrap();
        assert_eq!(base_font.as_name().unwrap(), name.as_bytes());
    }
}

#[test]
fn given_times_and_courier_when_measuring_then_times_kerns_by_afm_pairs_and_courier_is_fixed_width()
{
    let kerns = |font, text| {
        let style = TextStyle {
            font,
            ..TextStyle::plain(Pt(10.0))
        };
        operands(&parsed(&[Command::Text { text, style }]), "TJ")
    };
    // Times-Roman KPX A V -135, T a -80, and T agrave -40, which differs from the base letter's.
    assert_eq!(kerns(super::TIMES, "AV"), [[135.0]]);
    assert_eq!(kerns(super::TIMES, "Ta"), [[80.0]]);
    assert_eq!(kerns(super::TIMES, "T\u{e0}"), [[40.0]]);
    assert!(kerns(super::COURIER, "AV").is_empty());
    let courier = super::font::Font::Standard(super::COURIER.slot());
    assert_eq!((courier.advance(b"i"), courier.advance(b"W")), (600, 600));
}

fn marked(len: usize, marker: super::Marker) -> TextRun {
    TextRun {
        typography: super::Typography {
            marker,
            ..super::Typography::NORMAL
        },
        ..run(len, false, 10.0, false)
    }
}

#[test]
fn given_a_list_marker_when_its_first_line_moves_to_the_next_page_then_the_marker_goes_with_it() {
    // 8pt remain after the spacer, below one 9.25pt line.
    let runs = [
        marked(2, super::Marker::Outside),
        run(4, false, 10.0, false),
    ];
    let pdf = parsed(&[
        Command::Spacer(Pt(32.0)),
        Command::Paragraph {
            links: &[],
            align: TextAlign::Left,
            text: "1.item",
            runs: &runs,
        },
    ]);
    let pages = pdf.get_pages();
    let shown = |page: u32| String::from_utf8(pdf.get_page_content(pages[&page])).unwrap();
    assert_eq!(pages.len(), 2);
    assert!(!shown(1).contains("(1.)"));
    // The marker hangs left of the text by its 8.34pt width.
    assert!(shown(2).contains("(1.) Tj") && shown(2).contains("(item) Tj"));
    assert_eq!(shown(2).matches("42.82 Td").count(), 2);
    assert!(shown(2).contains("\n1.65"), "{}", shown(2));
}

#[test]
fn given_an_inside_marker_when_laid_out_then_the_first_line_starts_after_it() {
    let runs = [marked(2, super::Marker::Inside), run(4, false, 10.0, false)];
    let pdf = parsed(&[Command::Paragraph {
        links: &[],
        align: TextAlign::Left,
        text: "1.item",
        runs: &runs,
    }]);
    let starts: Vec<f32> = operands(&pdf, "Td").iter().map(|td| td[0]).collect();
    assert_eq!(starts.len(), 2);
    assert!(
        (starts[0] - 18.34).abs() < 0.001 && starts[1] == 10.0,
        "{starts:?}"
    );
}

#[test]
fn given_disc_circle_and_square_markers_when_painted_then_draw_shapes_instead_of_text() {
    let paragraphs: Vec<[TextRun; 2]> = (0..3)
        .map(|_| {
            [
                marked(3, super::Marker::Outside),
                run(1, false, 10.0, false),
            ]
        })
        .collect();
    let commands: Vec<Command> = ["\u{2022}x", "\u{25e6}x", "\u{25aa}x"]
        .iter()
        .zip(&paragraphs)
        .map(|(text, runs)| Command::Paragraph {
            links: &[],
            align: TextAlign::Left,
            text,
            runs: runs.as_slice(),
        })
        .collect();
    let pdf = parsed(&commands);
    let count = |name: &str| {
        operations(&pdf)
            .iter()
            .filter(|op| op.operator == name)
            .count()
    };
    // A disc is a 12-sided polygon and a fill, a ring two of them and an even-odd fill, a square four corners.
    let counts = ["m", "l", "h", "f", "f*"].map(count);
    assert_eq!(counts, [4, 36, 4, 2, 1]);
    assert_eq!(count("Tj"), 3);
}

#[test]
fn given_a_nowrap_word_wider_than_its_line_when_laid_out_then_rejects_where_a_normal_word_breaks() {
    let render = |nowrap| {
        let runs = [TextRun {
            typography: super::Typography {
                nowrap,
                ..super::Typography::NORMAL
            },
            ..run(12, false, 10.0, false)
        }];
        super::render(
            page(),
            &[Command::Paragraph {
                links: &[],
                align: TextAlign::Left,
                text: "WWWWWWWWWWWW",
                runs: &runs,
            }],
        )
    };
    assert_eq!(render(true), Err(RenderError::LineTooWide));
    assert!(render(false).is_ok());
}

#[test]
fn given_a_raised_run_when_painted_then_it_moves_up_by_the_rise_and_grows_the_line() {
    let raised = |rise| {
        let runs = [
            run(1, false, 10.0, false),
            TextRun {
                typography: super::Typography {
                    rise,
                    ..super::Typography::NORMAL
                },
                ..run(1, false, 10.0, false)
            },
        ];
        parsed(&[Command::Paragraph {
            links: &[],
            align: TextAlign::Left,
            text: "xy",
            runs: &runs,
        }])
    };
    let (flat, up) = (raised(0.0), raised(3.0));
    let starts = |pdf: &lopdf::Document| operands(pdf, "Td");
    // The second segment steps right by x's 5pt and up by the rise; the baseline drops by it too.
    assert_eq!(starts(&up)[1], [5.0, 3.0]);
    assert!((starts(&flat)[0][1] - starts(&up)[0][1] - 3.0).abs() < 0.001);
}

#[test]
fn given_a_run_background_when_painted_then_fills_its_text_extent_before_the_text() {
    let runs = [TextRun {
        decoration: super::RunDecoration {
            background: Some(Rgb {
                r: 255,
                g: 255,
                b: 0,
            }),
            ..Default::default()
        },
        ..run(2, false, 10.0, false)
    }];
    let pdf = parsed(&[Command::Paragraph {
        links: &[],
        align: TextAlign::Left,
        text: "ab",
        runs: &runs,
    }]);
    let baseline = operands(&pdf, "Td")[0][1];
    // The AFM ascender and descender give a 9.25pt box over the two 5.56pt glyphs.
    let (corner, edges) = (&operands(&pdf, "m")[0], operands(&pdf, "l"));
    assert!((corner[1] - (baseline - 2.07)).abs() < 0.001, "{corner:?}");
    assert!((edges[0][0] - corner[0] - 11.12).abs() < 0.001, "{edges:?}");
    assert!((edges[1][1] - corner[1] - 9.25).abs() < 0.001, "{edges:?}");
    let shown = content(&pdf);
    assert!(shown.find("1 1 0 rg").unwrap() < shown.find("BT").unwrap());
}

#[test]
fn given_a_word_split_across_two_colours_when_laid_out_then_its_pair_still_kerns() {
    let runs = [
        TextRun {
            color: Rgb { r: 255, g: 0, b: 0 },
            ..run(1, false, 10.0, false)
        },
        run(1, false, 10.0, false),
    ];
    let pdf = parsed(&[Command::Paragraph {
        links: &[],
        align: TextAlign::Left,
        text: "AV",
        runs: &runs,
    }]);
    // The V segment starts at A's 6.67pt advance less the 0.7pt kern.
    assert!((operands(&pdf, "Td")[1][0] - 5.97).abs() < 0.001);
}
