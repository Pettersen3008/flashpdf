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
            Edges::all(Pt::new(36.0).unwrap())
        ),
        Err(RenderError::InvalidPage)
    );
    let narrow_page = Page::new(
        Pt::new(72.0).unwrap(),
        Pt::new(100.0).unwrap(),
        Edges::all(Pt::new(10.0).unwrap()),
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
    Page::new(Pt(120.0), Pt(60.0), Edges::all(Pt(10.0))).unwrap()
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
        Command::RowStart {
            columns: &columns,
            slots: &[],
        },
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
        line_height: None,
        color: Rgb::BLACK,
        hard_break,
        decoration: Default::default(),
    }
}

/// A one-paragraph header or footer block.
fn line(text: &str, style: TextStyle) -> Vec<crate::OwnedCommand> {
    let run = TextRun {
        font: style.font,
        color: style.color,
        ..run(text.len(), false, style.size.get(), false)
    };
    vec![crate::OwnedCommand::Paragraph(
        style.align,
        text.into(),
        vec![run],
        Vec::new(),
    )]
}

#[test]
fn given_four_different_page_margins_and_a_header_and_footer_when_rendered_then_each_line_sits_inside_its_margins(
) {
    let margin = Edges {
        top: Pt(5.0),
        right: Pt(10.0),
        bottom: Pt(15.0),
        left: Pt(20.0),
    };
    let mut renderer = super::Renderer::new(Page::new(Pt(120.0), Pt(80.0), margin).unwrap());
    let right = TextStyle {
        align: TextAlign::Right,
        ..TextStyle::plain(Pt(10.0))
    };
    renderer
        .set_furniture(false, false, line("H", TextStyle::plain(Pt(10.0))))
        .unwrap();
    renderer
        .set_furniture(true, false, line("F", right))
        .unwrap();
    renderer.push(&[text("body")]).unwrap();
    let pdf = lopdf::Document::load_mem(&renderer.finish().unwrap()).unwrap();
    let starts: Vec<(f32, f32)> = content(&pdf)
        .lines()
        .filter_map(|line| line.strip_suffix(" Td")?.split_once(' '))
        .map(|(x, y)| (x.parse().unwrap(), y.parse().unwrap()))
        .collect();
    // 10pt Helvetica rises 7.18pt in a 9.25pt line, and "F" is 6.11pt wide.
    let expected = [
        (20.0, 80.0 - 5.0 - 9.25 - 7.18),
        (20.0, 80.0 - 5.0 - 7.18),
        (120.0 - 10.0 - 6.11, 15.0 + 9.25 - 7.18),
    ];
    assert_eq!(starts.len(), 3, "{starts:?}");
    for ((x, y), (left, baseline)) in starts.iter().zip(expected) {
        assert!(
            (x - left).abs() < 0.01 && (y - baseline).abs() < 0.01,
            "{starts:?}"
        );
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
        "BT\n10 33.57 Td\n(WWWW) Tj\n/F2 10 Tf\n37.76 0 Td\n(WW) Tj"
    );
}

#[test]
fn given_lines_in_one_style_across_two_pages_when_painted_then_each_page_sets_colour_and_font_once()
{
    let pdf = parsed(&[text("line"); 8]);
    let pages = pdf.get_pages();
    assert_eq!(pages.len(), 2);
    for page in pages.values() {
        let content = String::from_utf8(pdf.get_page_content(*page)).unwrap();
        assert_eq!(content.matches(" rg").count(), 1, "{content}");
        assert_eq!(content.matches(" Tf").count(), 1, "{content}");
    }
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

fn bordered() -> super::BoxStyle {
    super::BoxStyle {
        margin: Edges::all(Pt(0.0)),
        padding: Edges::all(Pt(0.0)),
        border: Edges::all(Pt(1.0)),
        background: Some(Rgb {
            r: 238,
            g: 238,
            b: 238,
        }),
        border_color: Edges::all(Rgb::BLACK),
    }
}

#[test]
fn given_a_bordered_box_taller_than_the_page_when_placed_then_splits_with_its_top_edge_first_and_bottom_edge_last(
) {
    let long = "word ".repeat(40);
    let pdf = parsed(&[
        Command::BoxStart { style: bordered() },
        text(&long),
        Command::BoxEnd,
    ]);
    let pages = pdf.get_pages();
    assert_eq!(pages.len(), 3);
    // Each page's fill spans the 100pt body width; only the first and last carry a 1pt horizontal edge.
    let edges: Vec<usize> = pages
        .values()
        .map(|id| {
            let content = String::from_utf8(pdf.get_page_content(*id)).unwrap();
            assert!(content.contains(" 100 "), "{content}");
            content.matches(" 100 1 re").count()
        })
        .collect();
    assert_eq!(edges, [1, 0, 1]);
    assert!(String::from_utf8(pdf.get_page_content(pages[&1]))
        .unwrap()
        .contains("10 49 100 1 re"));
    let text: String = (1..=3)
        .map(|page| pdf.extract_text(&[page]).unwrap())
        .collect();
    assert_eq!(text.matches("word").count(), 40);
}

#[test]
fn given_a_bordered_box_that_fits_a_page_when_it_crosses_the_page_end_then_moves_whole() {
    let pdf = parsed(&[
        text("first"),
        text("second"),
        text("third"),
        Command::BoxStart { style: bordered() },
        text("a"),
        text("b"),
        Command::BoxEnd,
    ]);
    assert_eq!(pdf.get_pages().len(), 2);
    assert_eq!(pdf.extract_text(&[2]).unwrap().trim(), "a\nb");
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
        Command::RowStart {
            columns: &columns,
            slots: &[],
        },
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
        Command::RowStart {
            columns: &columns,
            slots: &[],
        },
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
            Command::RowStart {
                columns: &columns,
                slots: &[]
            },
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
        commands.push(Command::RowStart {
            columns: &columns,
            slots: &[],
        });
        commands.extend(header.iter().map(|cell| text(cell)));
        commands.push(Command::RowEnd);
    }
    for row in cells.chunks(3) {
        commands.push(Command::RowStart {
            columns: &columns,
            slots: &[],
        });
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
        Command::RowStart {
            columns: &columns,
            slots: &[],
        },
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

const TOP: super::Slot = super::Slot::Cell {
    rows: 1,
    align: super::VerticalAlign::Top,
};

fn red_box() -> Command<'static> {
    Command::BoxStart {
        style: super::BoxStyle {
            margin: Edges::all(Pt(0.0)),
            padding: Edges::all(Pt(0.0)),
            border: Edges::all(Pt(0.0)),
            background: Some(Rgb { r: 255, g: 0, b: 0 }),
            border_color: Edges::all(Rgb::BLACK),
        },
    }
}

#[test]
fn given_a_cell_joined_to_the_next_column_when_laid_out_then_it_takes_both_widths() {
    use super::ColumnWidth::{Fixed, Fraction};
    let columns = [
        Fixed(Pt(20.0)),
        Fixed(Pt(30.0)),
        Fraction(FractionValue::new(1.0).unwrap()),
    ];
    let pdf = parsed(&[
        Command::RowStart {
            columns: &columns,
            slots: &[TOP, super::Slot::Join, TOP],
        },
        red_box(),
        text("a"),
        Command::BoxEnd,
        text("b"),
        Command::RowEnd,
    ]);
    let content = content(&pdf);
    assert!(content.contains("10 40.75 50 9.25 re"), "{content}");
    assert!(content.contains("60 42.82 Td"), "{content}");
}

#[test]
fn given_a_cell_spanning_two_rows_taller_than_both_when_laid_out_then_the_last_row_grows_to_fit_it()
{
    let columns = [super::ColumnWidth::Fraction(FractionValue::new(1.0).unwrap()); 2];
    let span = super::Slot::Cell {
        rows: 2,
        align: super::VerticalAlign::Top,
    };
    let pdf = parsed(&[
        Command::StackStart { gap: Pt(0.0) },
        Command::RowStart {
            columns: &columns,
            slots: &[span, TOP],
        },
        red_box(),
        Command::StackStart { gap: Pt(0.0) },
        text("a"),
        text("b"),
        text("c"),
        Command::StackEnd,
        Command::BoxEnd,
        text("r1"),
        Command::RowEnd,
        Command::RowStart {
            columns: &columns,
            slots: &[super::Slot::Covered, TOP],
        },
        text("r2"),
        Command::RowEnd,
        Command::StackEnd,
        text("after"),
    ]);
    let content = content(&pdf);
    // Three 9.25pt lines: the span's box covers both rows, and "after" follows them.
    assert!(content.contains("10 22.25 50 27.75 re"), "{content}");
    assert!(content.contains("60 33.57 Td"), "{content}");
    assert!(content.contains("10 15.07 Td"), "{content}");
}

#[test]
fn given_middle_and_bottom_cells_beside_a_taller_cell_when_laid_out_then_their_content_moves_down_by_half_and_all_of_the_free_space(
) {
    let columns = [super::ColumnWidth::Fraction(FractionValue::new(1.0).unwrap()); 3];
    let align = |align| super::Slot::Cell { rows: 1, align };
    let pdf = parsed(&[
        Command::RowStart {
            columns: &columns,
            slots: &[
                TOP,
                align(super::VerticalAlign::Middle),
                align(super::VerticalAlign::Bottom),
            ],
        },
        Command::StackStart { gap: Pt(0.0) },
        text("a"),
        text("b"),
        text("c"),
        Command::StackEnd,
        red_box(),
        text("m"),
        Command::BoxEnd,
        text("z"),
        Command::RowEnd,
    ]);
    let content = content(&pdf);
    // The middle cell's background still covers the row; only its text moves.
    assert!(
        content.contains("43.333332 22.25 33.333332 27.75 re"),
        "{content}"
    );
    assert!(content.contains("43.333332 33.57 Td"), "{content}");
    assert!(content.contains("76.666664 24.32 Td"), "{content}");
}

#[test]
fn given_a_table_row_taller_than_the_page_with_its_header_when_rendered_then_rejects_naming_the_row(
) {
    let columns = [super::ColumnWidth::Fraction(
        FractionValue::new(1.0).unwrap(),
    )];
    let mut commands = vec![
        table_start(1),
        Command::RowStart {
            columns: &columns,
            slots: &[],
        },
        text("header"),
        Command::RowEnd,
        Command::RowStart {
            columns: &columns,
            slots: &[],
        },
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
        super::font::Font::Helvetica { bold: false }.advance(b"A"),
        super::font::Font::Helvetica { bold: true }.advance(b"A"),
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
        vec![
            Command::RowStart {
                columns: &[],
                slots: &[],
            },
            Command::RowEnd,
        ],
        vec![
            Command::RowStart {
                columns: &[Fixed(Pt(101.0))],
                slots: &[],
            },
            text("x"),
            Command::RowEnd,
        ],
        vec![
            Command::RowStart {
                columns: &[Fixed(Pt(20.0))],
                slots: &[],
            },
            Command::RowEnd,
        ],
        vec![
            Command::RowStart {
                columns: &[Fixed(Pt(20.0))],
                slots: &[],
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
                slots: &[],
            },
            text("x"),
            Command::RowEnd,
            Command::RowStart {
                columns: &[Fixed(Pt(20.0)), Fixed(Pt(20.0))],
                slots: &[],
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
        super::render(
            page(),
            &[Command::RowStart {
                columns: &too_many,
                slots: &[]
            }]
        ),
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
            font: super::FontId::new(2),
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
    assert_eq!(pdf.extract_text(&[1]).unwrap().trim(), text);
    let page = *pdf.get_pages().values().next().unwrap();
    let font = &pdf.get_page_fonts(page).unwrap()[b"F3".as_slice()];
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
    assert_eq!(pdf.extract_text(&[1]).unwrap().trim(), "a\nb");
    assert_eq!(pdf.extract_text(&[2]).unwrap().trim(), "c");
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
    for cell in ["H", "F"]
        .into_iter()
        .chain(cells.iter().map(String::as_str))
    {
        commands.extend([
            Command::RowStart {
                columns: &columns,
                slots: &[],
            },
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
        assert_eq!(pdf.extract_text(&[page]).unwrap().trim(), expected);
    }
}

#[test]
fn given_a_header_and_a_line_taller_than_the_page_when_rendered_then_rejects_with_page_overflow() {
    let (sender, receiver) = std::sync::mpsc::channel();
    // A thread, so a pagination loop that never ends fails the test instead of hanging it.
    std::thread::spawn(move || {
        let mut renderer = super::Renderer::new(page());
        renderer
            .set_furniture(false, false, line("Header", TextStyle::plain(Pt(10.0))))
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
        for footer in [false, true] {
            renderer
                .set_furniture(footer, false, line("x", TextStyle::plain(Pt(10.0))))
                .unwrap();
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
    let mut table = vec![
        table_start(0),
        Command::RowStart {
            columns: &columns,
            slots: &[],
        },
    ];
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
        .set_furniture(
            true,
            false,
            line(&"x".repeat(60_000), TextStyle::plain(Pt(0.01))),
        )
        .unwrap();
    renderer.push(&vec![Command::PageBreak; 300]).unwrap();
    assert_eq!(renderer.finish().err(), Some(RenderError::DocumentTooLarge));
}

#[test]
fn given_a_two_line_first_page_header_and_a_numbered_header_when_rendering_then_each_page_paints_and_reserves_its_own(
) {
    let mut renderer = super::Renderer::new(page());
    let plain = TextStyle::plain(Pt(10.0));
    renderer
        .set_furniture(false, false, line("Later \u{1e}", plain))
        .unwrap();
    let runs = vec![run(5, false, 10.0, true), run(4, false, 10.0, false)];
    let first = crate::OwnedCommand::Paragraph(TextAlign::Left, "Firstpage".into(), runs, vec![]);
    renderer.set_furniture(false, true, vec![first]).unwrap();
    renderer.push(&["a", "b", "c", "d", "e"].map(text)).unwrap();
    let pdf = lopdf::Document::load_mem(&renderer.finish().unwrap()).unwrap();
    // 40pt of body keeps 21.5pt under the 18.5pt first header and 30.75pt under the later one.
    assert_eq!(pdf.extract_text(&[1]).unwrap().trim(), "a\nb\nFirst\npage");
    assert_eq!(pdf.extract_text(&[2]).unwrap().trim(), "c\nd\ne\nLater 2");
}

#[test]
fn given_a_header_and_first_page_footer_over_half_the_page_between_margins_when_set_then_rejects_with_page_overflow(
) {
    let mut renderer = super::Renderer::new(page());
    renderer
        .set_furniture(false, false, line("x", TextStyle::plain(Pt(10.0))))
        .unwrap();
    // 9.25pt plus 11.1pt passes half of the 40pt between the margins.
    assert_eq!(
        renderer.set_furniture(true, true, line("x", TextStyle::plain(Pt(12.0)))),
        Err(RenderError::PageOverflow)
    );
}

#[test]
fn given_a_page_number_whose_real_width_pulls_a_larger_word_up_a_line_when_finishing_then_rejects_with_page_overflow(
) {
    let page = Page::new(Pt(100.0), Pt(200.0), Edges::all(Pt(5.0))).unwrap();
    let mut renderer = super::Renderer::new(page);
    // In 90pt, ten 10pt digits push the first 40pt "M" to a second line; "1" lets it
    // stay and pushes the second one down, so two 37pt lines replace 9.25pt and 37pt.
    let runs = vec![run(1, false, 10.0, false), run(4, false, 40.0, false)];
    let footer = crate::OwnedCommand::Paragraph(TextAlign::Left, "\u{1e} M M".into(), runs, vec![]);
    renderer.set_furniture(true, false, vec![footer]).unwrap();
    renderer.push(&[text("body")]).unwrap();
    assert_eq!(renderer.finish().err(), Some(RenderError::PageOverflow));
}
