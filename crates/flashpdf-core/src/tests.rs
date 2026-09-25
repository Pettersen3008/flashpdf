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
    assert_eq!(
        pdf.extract_text(&[1]).unwrap().trim(),
        "First \u{20ac}\nAfter spacer"
    );
    assert_eq!(pdf.extract_text(&[2]).unwrap().trim(), "Second page");
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
        let Ok(font) = super::EmbeddedFont::parse(bytes) else {
            continue;
        };
        // Deflating the font program dominates debug time; one in ten covers pdf.rs.
        if round % 10 != 0 {
            continue;
        }
        let mut renderer = super::Renderer::with_fonts(Page::A4, vec![font]);
        let style = TextStyle {
            font: super::FontId::new(2),
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
    assert_eq!(pdf.extract_text(&[1]).unwrap().trim(), "first");
    assert_eq!(pdf.extract_text(&[2]).unwrap().trim(), "a\nb");
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
    assert_eq!(
        pdf.extract_text(&[1]).unwrap().trim(),
        "first\nWWWWW001\nWWWWW002\nWWWWW003"
    );
    assert!(pdf.extract_text(&[76]).unwrap().trim().ends_with("last"));
}

#[test]
fn given_a_word_wider_than_the_column_when_wrapping_then_breaks_it_at_the_last_fitting_glyph() {
    // "i" is 2.22pt, "W" 9.44pt; the run fits 100pt only when split by glyph.
    let pdf = parsed(&[text(&format!("ab {}WW cd", "i".repeat(50)))]);
    assert_eq!(
        pdf.extract_text(&[1]).unwrap().trim(),
        format!("ab\n{}\n{}WW cd", "i".repeat(45), "i".repeat(5))
    );
    let pdf = parsed(&[text(&"W".repeat(25))]);
    assert_eq!(
        pdf.extract_text(&[1]).unwrap().trim(),
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
        color: Rgb::BLACK,
        hard_break,
        decoration: Default::default(),
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
    assert_eq!(pdf.extract_text(&[1]).unwrap().trim(), "WWWWW\nWWWWWW");
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
    assert_eq!(pdf.extract_text(&[1]).unwrap().trim(), "a\nb");
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
    assert_eq!(
        pdf.extract_text(&[1]).unwrap().trim(),
        "aa\naa\nbb\nbb\nlast\nafter"
    );
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
        let page_text = pdf.extract_text(&[page]).unwrap();
        let rest = page_text
            .trim()
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
    assert_eq!(
        pdf.extract_text(&[1]).unwrap().trim(),
        "header\nline\nline\nline"
    );
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
    assert_ne!(
        super::font::Font::Helvetica { bold: false }
            .width(b'A')
            .unwrap(),
        super::font::Font::Helvetica { bold: true }
            .width(b'A')
            .unwrap(),
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
    assert!(lopdf::Document::load_mem(&renderer.finish()).is_ok());
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
