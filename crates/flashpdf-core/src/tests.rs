use super::{
    Command, Edges, Fraction as FractionValue, LineStyle, Page, Percent as PercentValue, Pt, Radii,
    RenderError, Rgb, TextAlign, TextRun, TextStyle, OPAQUE,
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
    assert_eq!(page_text(&pdf, 1), "First \u{20ac}\nAfter spacer");
    assert_eq!(page_text(&pdf, 2), "Second page");
    assert!(String::from_utf8(pdf.get_page_content((4, 0)))
        .unwrap()
        .contains("36 793.09717 Td"));
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
        "BT\n0 0 0 rg\n/F1 10 Tf\n36 796.7837 Td\n(hello) Tj\nET"
    );
}

#[test]
fn given_content_beyond_the_cap_when_pushing_then_rejects() {
    let mut renderer = super::Renderer::new(Page::A4);
    // 10pt text fills pages densely enough to reach the byte cap before the page cap.
    let long = Command::Text {
        text: &"W ".repeat(30_000),
        style: TextStyle::plain(Pt(10.0)),
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
    Page::new(Pt(120.0), Pt(60.0), Edges::all(Pt(10.0))).unwrap()
}

fn text(text: &str) -> Command<'_> {
    Command::Text {
        text,
        style: TextStyle::plain(Pt(10.0)),
    }
}

fn fixed_line(text: &str) -> Command<'_> {
    Command::Text {
        text,
        // A 9.25pt normal line keeps four lines and a border inside the 40pt body.
        style: TextStyle::plain(Pt(9.25 * 2048.0 / 2355.0)),
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
    let pdf = parsed(&[Command::Spacer(Pt(28.0)), text("bottom")]);
    assert_eq!(pdf.get_pages().len(), 1);
    assert!(String::from_utf8(pdf.get_page_content((4, 0)))
        .unwrap()
        .contains("10 12.783691 Td"));
}

#[test]
fn given_exact_bottom_stack_when_placed_then_stays_on_page() {
    let pdf = parsed(&[
        Command::StackStart { gap: Pt(16.0) },
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
        Command::Spacer(Pt(40.0 - 2.0 * 2355.0 * 10.0 / 2048.0)),
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
        Command::StackStart { gap: Pt(16.0) },
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

#[test]
fn given_a_margin_past_the_page_end_when_placed_then_the_next_page_starts_without_it() {
    // 28.5pt remain after the first line; the 35pt margin fits an empty page.
    let pdf = parsed(&[text("a"), Command::Spacer(Pt(35.0)), text("b")]);
    let alone = parsed(&[text("b")]);
    let pages = pdf.get_pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(page_text(&pdf, 2), "b");
    assert_eq!(
        baselines(&pdf, pages[&2]),
        baselines(&alone, alone.get_pages()[&1])
    );
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
    // 40pt of body holds three 11.5pt lines; 302 lines then need 101 pages.
    let pages = pdf.get_pages();
    assert_eq!(pages.len(), 101);
    let mut total = 0;
    for id in pages.values() {
        let ys = baselines(&pdf, *id);
        total += ys.len();
        assert!(ys.windows(2).all(|pair| pair[0] - pair[1] > 11.4));
        assert!(ys.iter().all(|y| *y > 12.0));
    }
    assert_eq!(total, 302);
    assert_eq!(page_text(&pdf, 1), "first\nWWWWW001\nWWWWW002");
    assert!(page_text(&pdf, 101).ends_with("last"));
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

// "W" is 9.44pt, "-" 3.33pt and "!" 2.78pt in a 100pt column, so twelve "W"s never share a line.
#[test]
fn given_a_hyphenated_word_when_wrapping_then_breaks_after_the_hyphen() {
    let pdf = parsed(&[text("WWWWWW-WWWWWW")]);
    assert_eq!(page_text(&pdf, 1), "WWWWWW-\nWWWWWW");
}

#[test]
fn given_a_word_ending_in_an_exclamation_mark_when_wrapping_then_keeps_the_mark_with_the_word() {
    // "WWWWWW-WWWW" fits alone; with its "!" it does not.
    let pdf = parsed(&[text("WWWWWW-WWWW!")]);
    assert_eq!(page_text(&pdf, 1), "WWWWWW-\nWWWW!");
}

#[test]
fn given_a_url_when_wrapping_then_breaks_after_a_slash() {
    let pdf = parsed(&[text("x.io/WWWWWWWWWW")]);
    assert_eq!(page_text(&pdf, 1), "x.io/\nWWWWWWWWWW");
}

#[test]
fn given_soft_hyphens_when_wrapping_then_shows_a_hyphen_only_where_the_line_breaks() {
    let pdf = parsed(&[text("WWWWWW\u{ad}WWWWWW WW\u{ad}W")]);
    assert_eq!(page_text(&pdf, 1), "WWWWWW-\nWWWWWW WWW");
}

#[test]
fn given_a_zero_width_space_and_a_word_joiner_when_wrapping_then_only_the_space_breaks_and_neither_paints(
) {
    let pdf = parsed(&[text("WWWWWW\u{200b}WWWWWW")]);
    assert_eq!(page_text(&pdf, 1), "WWWWWW\nWWWWWW");
    let pdf = parsed(&[text("WWWWWW-\u{2060}WWWWWW")]);
    assert_eq!(page_text(&pdf, 1), "WWWWWW-WWWW\nWW");
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

/// A one-paragraph header or footer block.
fn line(text: &str, style: TextStyle) -> Vec<crate::OwnedCommand> {
    let run = TextRun {
        font: style.font,
        color: style.color,
        ..run(text.len(), false, style.size.get(), false)
    };
    vec![crate::OwnedCommand::Paragraph(
        style.align,
        [2, 2],
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
    // Arial metrics give Helvetica an 11.499023pt line and a 9.216309pt ascent; "F" is 6.11pt wide.
    let expected = [
        (20.0, 80.0 - 5.0 - 11.499023 - 9.216309),
        (20.0, 80.0 - 5.0 - 9.216309),
        (120.0 - 10.0 - 6.11, 15.0 + 11.499023 - 9.216309),
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
        orphans: 2,
        widows: 2,
        text: "WWWWW WWWWWW",
        runs: &runs,
    }]);
    assert_eq!(page_text(&pdf, 1), "WWWWW\nWWWWWW");
    let content = content(&pdf);
    let objects: Vec<&str> = content.split("ET").collect();
    assert_eq!(objects.len(), 3, "{content}");
    assert_eq!(
        objects[1].trim(),
        "BT\n10 29.284668 Td\n(WWWW) Tj\n/F2 10 Tf\n37.76 0 Td\n(WW) Tj"
    );
}

#[test]
fn given_lines_in_one_style_across_two_pages_when_painted_then_each_page_sets_colour_and_font_once()
{
    let pdf = parsed(&[text("line"); 6]);
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
        orphans: 2,
        widows: 2,
        text: "a b",
        runs: &runs,
    }]);
    assert_eq!(page_text(&pdf, 1), "a\nb");
    assert_eq!(baselines(&pdf, (4, 0)), [40.78369, 29.284668]);
}

#[test]
fn given_the_built_in_fonts_when_laying_out_two_lines_then_normal_line_height_is_the_browser_fonts()
{
    // Arial, Times New Roman, and Courier New have hhea lines of 2355, 2355, and 2320 of 2048 units.
    for (font, units) in [
        (super::HELVETICA, 2355.0),
        (super::TIMES_BOLD, 2355.0),
        (super::COURIER_OBLIQUE, 2320.0),
    ] {
        let runs = [
            TextRun {
                font,
                ..run(1, false, 10.0, true)
            },
            TextRun {
                font,
                ..run(1, false, 10.0, false)
            },
        ];
        let pdf = parsed(&[Command::Paragraph {
            orphans: 2,
            widows: 2,
            links: &[],
            align: TextAlign::Left,
            text: "ab",
            runs: &runs,
        }]);
        let lines = baselines(&pdf, (4, 0));
        assert!(
            (lines[0] - lines[1] - units * 10.0 / 2048.0).abs() < 0.001,
            "{lines:?}"
        );
    }
}

#[test]
fn given_mixed_sizes_on_one_line_when_laying_out_then_the_line_takes_the_larger_metrics() {
    let runs = [run(1, false, 10.0, false), run(1, false, 20.0, false)];
    let pdf = parsed(&[
        Command::Paragraph {
            links: &[],
            align: TextAlign::Left,
            orphans: 2,
            widows: 2,
            text: "ab",
            runs: &runs,
        },
        text("c"),
    ]);
    // Baseline sits at the 20pt ascent (18.43); the next block starts after the 20pt line height (23).
    assert_eq!(baselines(&pdf, (4, 0)), [31.567383, 17.785645]);
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
                orphans: 2,
                widows: 2,
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
        border_style: Edges::all(LineStyle::Solid),
        radius: None,
        alpha: OPAQUE,
    }
}

#[test]
fn given_a_bordered_box_taller_than_the_page_when_placed_then_splits_with_its_top_edge_first_and_bottom_edge_last(
) {
    let long = "word ".repeat(40);
    let pdf = parsed(&[
        Command::BoxStart {
            style: bordered(),
            size: crate::BoxSize::AUTO,
        },
        text(&long),
        Command::BoxEnd,
    ]);
    let pages = pdf.get_pages();
    assert_eq!(pages.len(), 4);
    // Each page's fill spans the 100pt body width; only the first and last carry a 1pt horizontal edge.
    let edges: Vec<usize> = pages
        .values()
        .map(|id| {
            let content = String::from_utf8(pdf.get_page_content(*id)).unwrap();
            assert!(content.contains(" 100 "), "{content}");
            content.matches(" 100 1 re").count()
        })
        .collect();
    assert_eq!(edges, [1, 0, 0, 1]);
    assert!(String::from_utf8(pdf.get_page_content(pages[&1]))
        .unwrap()
        .contains("10 49 100 1 re"));
    let text: String = (1..=4)
        .map(|page| pdf.extract_text(&[page]).unwrap())
        .collect();
    assert_eq!(text.matches("word").count(), 40);
}

fn page_texts(pdf: &lopdf::Document) -> Vec<String> {
    (1..=pdf.get_pages().len() as u32)
        .map(|page| pdf.extract_text(&[page]).unwrap().trim().to_owned())
        .collect()
}

#[test]
fn given_a_bordered_box_that_crosses_the_page_end_when_placed_then_breaks_there_with_one_edge_per_page(
) {
    let pdf = parsed(&[
        fixed_line("first"),
        fixed_line("second"),
        fixed_line("third"),
        Command::BoxStart {
            style: bordered(),
            size: crate::BoxSize::AUTO,
        },
        fixed_line("a"),
        fixed_line("b"),
        Command::BoxEnd,
    ]);
    assert_eq!(page_texts(&pdf), ["first\nsecond\nthird\na", "b"]);
    for id in pdf.get_pages().values() {
        let content = String::from_utf8(pdf.get_page_content(*id)).unwrap();
        assert_eq!(content.matches(" 100 1 re").count(), 1, "{content}");
    }
}

#[test]
fn given_a_box_whose_first_line_does_not_fit_below_its_top_edge_when_placed_then_moves_whole() {
    let style = super::BoxStyle {
        padding: Edges::all(Pt(3.0)),
        ..bordered()
    };
    let pdf = parsed(&[
        text("first"),
        text("second"),
        text("third"),
        Command::BoxStart {
            style,
            size: crate::BoxSize::AUTO,
        },
        text("a"),
        Command::BoxEnd,
    ]);
    assert_eq!(page_texts(&pdf), ["first\nsecond\nthird", "a"]);
}

#[test]
fn given_a_box_whose_bottom_edge_does_not_fit_below_its_last_line_when_closed_then_the_line_moves_with_it(
) {
    let style = super::BoxStyle {
        padding: Edges::all(Pt(1.0)),
        ..bordered()
    };
    let pdf = parsed(&[
        text("first"),
        text("second"),
        Command::BoxStart {
            style,
            size: crate::BoxSize::AUTO,
        },
        text("a"),
        text("b"),
        Command::BoxEnd,
    ]);
    assert_eq!(page_texts(&pdf), ["first\nsecond\na", "b"]);
}

/// Four lines of one 66pt word each, keeping `keep` orphans and widows.
fn four_lines(runs: &[TextRun; 1], keep: u16) -> Command<'_> {
    Command::Paragraph {
        align: TextAlign::Left,
        orphans: keep,
        widows: keep,
        text: FOUR_WORDS,
        runs,
        links: &[],
    }
}

const FOUR_WORDS: &str = "WWWWWWW WWWWWWW WWWWWWW WWWWWWW";

#[test]
fn given_room_for_one_line_of_a_four_line_paragraph_when_breaking_then_two_orphans_move_it_whole_and_one_splits_it(
) {
    let runs = [TextRun {
        line_height: Some(Pt(10.0)),
        ..run(FOUR_WORDS.len(), false, 10.0, false)
    }];
    let lines = |keep| {
        parsed(&[
            fixed_line("first"),
            fixed_line("second"),
            fixed_line("third"),
            four_lines(&runs, keep),
        ])
    };
    let w = "WWWWWWW";
    assert_eq!(
        page_texts(&lines(2)),
        ["first\nsecond\nthird".to_owned(), [w; 4].join("\n")]
    );
    assert_eq!(
        page_texts(&lines(1)),
        [format!("first\nsecond\nthird\n{w}"), [w; 3].join("\n")]
    );
}

#[test]
fn given_room_for_three_lines_of_a_four_line_paragraph_when_breaking_then_two_widows_go_to_the_next_page(
) {
    let runs = [run(FOUR_WORDS.len(), false, 10.0, false)];
    let pdf = parsed(&[text("first"), four_lines(&runs, 2)]);
    let two = ["WWWWWWW"; 2].join("\n");
    assert_eq!(page_texts(&pdf), [format!("first\n{two}"), two]);
}

#[test]
fn given_a_heading_that_avoids_a_break_after_it_when_the_next_paragraph_moves_then_the_heading_moves_with_it(
) {
    let pdf = parsed(&[
        fixed_line("first"),
        fixed_line("second"),
        fixed_line("Heading"),
        Command::Avoid,
        fixed_line(&"WWWWWWW ".repeat(3)),
    ]);
    assert_eq!(
        page_texts(&pdf),
        ["first\nsecond", "Heading\nWWWWWWW\nWWWWWWW\nWWWWWWW"]
    );
}

#[test]
fn given_an_avoided_break_after_a_page_filling_paragraph_when_the_next_block_moves_then_its_last_two_lines_move_too(
) {
    let runs = [run(FOUR_WORDS.len(), false, 10.0, false)];
    let pdf = parsed(&[four_lines(&runs, 2), Command::Avoid, text("next")]);
    let two = ["WWWWWWW"; 2].join("\n");
    assert_eq!(page_texts(&pdf), [two.clone(), format!("{two}\nnext")]);
}

#[test]
fn given_a_chain_of_avoided_breaks_taller_than_a_page_when_placed_then_breaks_anyway() {
    let mut commands = Vec::new();
    for line in ["a", "b", "c", "d", "e"] {
        commands.extend([fixed_line(line), Command::Avoid]);
    }
    assert_eq!(page_texts(&parsed(&commands)), ["a\nb\nc\nd", "e"]);
}

#[test]
fn given_a_margin_that_does_not_fit_at_a_page_end_when_the_next_line_moves_then_the_next_page_starts_with_the_line(
) {
    let pdf = parsed(&[
        text("first"),
        text("second"),
        text("third"),
        Command::Spacer(Pt(13.0)),
        text("next"),
    ]);
    // The 40pt body starts at y 50, with a 9.216309pt ascent.
    assert_eq!(baselines(&pdf, pdf.get_pages()[&2]), [40.78369]);
}

#[test]
fn given_a_forced_break_with_nothing_above_it_when_placed_then_adds_no_blank_page() {
    let pdf = parsed(&[
        Command::PageBreak,
        text("a"),
        Command::PageBreak,
        Command::PageBreak,
        text("b"),
    ]);
    assert_eq!(page_texts(&pdf), ["a", "b"]);
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
    assert_eq!(page_text(&pdf, 1), "aa\naa\nbb\nbb\nlast\nafter");
    let content = String::from_utf8(pdf.get_page_content((4, 0))).unwrap();
    for position in [
        "10 40.78369 Td",
        "10 29.284668 Td",
        "30 40.78369 Td",
        "30 29.284668 Td",
        "50 40.78369 Td",
        "10 17.785645 Td",
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
    assert!(content.contains("10 40.78369 Td"));
    assert!(content.contains("60 40.78369 Td"));
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
    // 40pt of body holds the 23pt header plus one 11.5pt row.
    assert_eq!(pdf.get_pages().len(), 60);
    let mut body = Vec::new();
    for page in 1..=60 {
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
        border_style: Edges::all(LineStyle::Solid),
        radius: None,
        alpha: OPAQUE,
    };
    let columns = [Fixed(Pt(50.0)), Fraction(FractionValue::new(1.0).unwrap())];
    let pdf = parsed(&[
        Command::RowStart {
            columns: &columns,
            slots: &[],
        },
        Command::BoxStart {
            style,
            size: crate::BoxSize::AUTO,
        },
        text("a"),
        Command::BoxEnd,
        Command::StackStart { gap: Pt(0.0) },
        text("a"),
        text("b"),
        Command::StackEnd,
        Command::RowEnd,
    ]);
    assert!(
        content(&pdf).contains("10 27.001953 50 22.998047 re"),
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
        size: crate::BoxSize::AUTO,
        style: super::BoxStyle {
            margin: Edges::all(Pt(0.0)),
            padding: Edges::all(Pt(0.0)),
            border: Edges::all(Pt(0.0)),
            background: Some(Rgb { r: 255, g: 0, b: 0 }),
            border_color: Edges::all(Rgb::BLACK),
            border_style: Edges::all(LineStyle::Solid),
            radius: None,
            alpha: OPAQUE,
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
    assert!(
        content.contains("10 38.500977 50 11.499023 re"),
        "{content}"
    );
    assert!(content.contains("60 40.78369 Td"), "{content}");
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
    // Three 11.499023pt lines fill the span; the following line starts a new page.
    assert!(content.contains("10 15.50293 50 34.49707 re"), "{content}");
    assert!(content.contains("60 29.284668 Td"), "{content}");
    assert_eq!(page_text(&pdf, 2), "after");
    assert_eq!(baselines(&pdf, pdf.get_pages()[&2]), [40.78369]);
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
        content.contains("43.333332 15.50293 33.333332 34.49707 re"),
        "{content}"
    );
    assert!(content.contains("43.333332 29.284668 Td"), "{content}");
    assert!(content.contains("76.666664 17.785645 Td"), "{content}");
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
    // Two lines under a one-line header fit an empty page, so the row moves whole.
    commands.drain(6..8);
    let pdf = parsed(&commands);
    assert_eq!(pdf.get_pages().len(), 1);
    assert_eq!(page_text(&pdf, 1), "header\nline\nline");
}

#[test]
fn given_nested_backgrounds_when_rendered_then_parent_paint_precedes_child_paint() {
    let red = super::BoxStyle {
        margin: Edges::all(Pt(0.0)),
        padding: Edges::all(Pt(1.0)),
        border: Edges::all(Pt(0.0)),
        background: Some(Rgb { r: 255, g: 0, b: 0 }),
        border_color: Edges::all(Rgb::BLACK),
        border_style: Edges::all(LineStyle::Solid),
        radius: None,
        alpha: OPAQUE,
    };
    let blue = super::BoxStyle {
        background: Some(Rgb { r: 0, g: 0, b: 255 }),
        ..red
    };
    let pdf = parsed(&[
        Command::BoxStart {
            style: red,
            size: crate::BoxSize::AUTO,
        },
        Command::BoxStart {
            style: blue,
            size: crate::BoxSize::AUTO,
        },
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
    // 40pt of content holds three normal 11.5pt lines, but only two 15pt ones.
    let runs = [true, true, false].map(|hard_break| TextRun {
        line_height: Some(Pt(15.0)),
        ..run(1, false, 10.0, hard_break)
    });
    let pdf = parsed(&[Command::Paragraph {
        links: &[],
        align: TextAlign::Left,
        orphans: 2,
        widows: 2,
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
    // 40pt of body holds the 11.5pt header and footer plus one 11.5pt row.
    assert_eq!(pdf.get_pages().len(), 6);
    for (page, rows) in (1..=6).zip(cells.chunks(1)) {
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
            .set_furniture(false, false, line("Header", TextStyle::plain(Pt(10.0))))
            .unwrap();
        let runs = [TextRun {
            line_height: Some(Pt(100.0)),
            ..run(1, false, 10.0, false)
        }];
        let _ = sender.send(renderer.push(&[Command::Paragraph {
            links: &[],
            align: TextAlign::Left,
            orphans: 2,
            widows: 2,
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
                .set_furniture(footer, false, line("x", TextStyle::plain(Pt(8.0))))
                .unwrap();
        }
        renderer.push(commands)?;
        renderer.finish()
    };
    // Two 9.2pt furniture lines leave room for one 11.5pt body line, not three.
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
    stack.drain(1..3);
    let pdf = lopdf::Document::load_mem(&render(&stack).unwrap()).unwrap();
    assert_eq!(pdf.get_pages().len(), 1);
}

#[test]
fn given_two_sizes_with_the_same_line_height_on_one_line_when_wrapping_then_the_line_box_spans_the_tallest_ascent_and_deepest_depth(
) {
    // In a 20pt line, 20pt text rises 16.93pt and 10pt text sinks 6.533pt below the baseline.
    let runs = [20.0, 10.0].map(|size| TextRun {
        line_height: Some(Pt(20.0)),
        ..run(1, false, size, false)
    });
    let pdf = parsed(&[
        Command::Paragraph {
            links: &[],
            align: TextAlign::Left,
            orphans: 2,
            widows: 2,
            text: "ab",
            runs: &runs,
        },
        text("next"),
    ]);
    // The 23.467pt line box puts the next baseline at 50 - 23.467 - 9.216.
    let next = baselines(&pdf, (4, 0))[1];
    assert!((next - 17.317).abs() < 0.001, "{next}");
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
    renderer
        .push(&[text("x"), Command::PageBreak].repeat(300))
        .unwrap();
    assert_eq!(renderer.finish().err(), Some(RenderError::DocumentTooLarge));
}

fn decorated(border: f32) -> super::BoxStyle {
    super::BoxStyle {
        margin: Edges::all(Pt(0.0)),
        padding: Edges::all(Pt(1.0)),
        border: Edges::all(Pt(border)),
        background: Some(Rgb { r: 255, g: 0, b: 0 }),
        border_color: Edges::all(Rgb::BLACK),
        border_style: Edges::all(LineStyle::Solid),
        radius: None,
        alpha: OPAQUE,
    }
}

fn round(value: f32, percent: u8) -> Option<Radii> {
    Some(Radii {
        values: [value; 8],
        percent,
    })
}

fn boxed(style: super::BoxStyle) -> lopdf::Document {
    parsed(&[
        Command::BoxStart {
            size: crate::BoxSize::AUTO,
            style,
        },
        text("x"),
        Command::BoxEnd,
    ])
}

#[test]
fn given_a_rounded_box_with_a_solid_border_when_rendered_then_strokes_the_centre_line_over_the_filled_outline(
) {
    let style = super::BoxStyle {
        radius: round(4.0, 0),
        ..decorated(1.0)
    };
    let stream = content(&boxed(style));
    // The background and the border each have four corners, and a solid border has no dashes.
    assert_eq!(stream.matches(" c\n").count(), 8, "{stream}");
    assert!(
        stream.contains("\nS\n") && !stream.contains(" d\n"),
        "{stream}"
    );
    assert!(!stream.contains(" re\n"), "{stream}");
}

#[test]
fn given_a_dotted_border_with_a_radius_when_rendered_then_strokes_round_capped_zero_length_dashes()
{
    let style = super::BoxStyle {
        radius: round(4.0, 0),
        border_style: Edges::all(LineStyle::Dotted),
        ..decorated(2.0)
    };
    let stream = content(&boxed(style));
    assert!(
        stream.contains("1 J\n") && stream.contains("[0 "),
        "{stream}"
    );
    assert!(
        stream.contains("] 0 d\n") && stream.contains("S\n"),
        "{stream}"
    );
}

#[test]
fn given_a_dashed_side_when_rendered_then_strokes_a_line_whose_gap_fits_whole_dashes() {
    let style = super::BoxStyle {
        border_style: Edges {
            top: LineStyle::Dashed,
            ..Edges::all(LineStyle::Solid)
        },
        ..decorated(1.0)
    };
    let stream = content(&boxed(style));
    // A 100pt side takes 20 dashes of 3pt, so the 19 gaps stretch from 2pt to 2.105pt.
    assert!(stream.contains("[3 2.105"), "{stream}");
    assert!(!stream.contains(" J\n"), "{stream}");
    // The three solid sides and the background stay rectangles.
    assert_eq!(stream.matches(" re\nf\n").count(), 4, "{stream}");
}

#[test]
fn given_percent_radii_when_rendered_then_they_resolve_against_the_box() {
    let style = super::BoxStyle {
        radius: round(50.0, 0xff),
        ..decorated(0.0)
    };
    // 50% resolves against the box including padding and the browser line height.
    let stream = content(&boxed(style));
    assert!(
        stream.contains("10 43.25049 m\n10 46.97814 32.38576 50 60 50 c\n60 50 l\n"),
        "{stream}"
    );
}

#[test]
fn given_radii_far_beyond_the_box_when_rendered_then_they_shrink_to_a_pill_without_overflow() {
    let style = super::BoxStyle {
        radius: round(3.402_82e38, 0),
        ..decorated(1.0)
    };
    let stream = content(&boxed(style));
    assert!(
        !stream.contains("NaN") && !stream.contains("inf"),
        "{stream}"
    );
    assert_eq!(stream.matches(" c\n").count(), 8, "{stream}");
}

#[test]
fn given_a_translucent_box_and_text_when_rendered_then_each_sets_its_alpha_and_the_page_defines_it()
{
    let style = super::BoxStyle {
        alpha: 128,
        ..decorated(1.0)
    };
    let run = TextRun {
        decoration: super::RunDecoration {
            alpha: 128,
            ..Default::default()
        },
        ..run(1, false, 10.0, false)
    };
    let commands = [
        Command::BoxStart {
            size: crate::BoxSize::AUTO,
            style,
        },
        Command::Paragraph {
            orphans: 2,
            widows: 2,
            links: &[],
            align: TextAlign::Left,
            text: "x",
            runs: &[run],
        },
        Command::BoxEnd,
    ];
    let bytes = super::render(page(), &commands).unwrap();
    let stream = content(&lopdf::Document::load_mem(&bytes).unwrap());
    // The background restores opaque graphics before the text sets its own alpha.
    assert_eq!(stream.matches("/G8080 gs\n").count(), 2, "{stream}");
    assert!(stream.trim_end().ends_with("/GFFFF gs"), "{stream}");
    // One stroked outline replaces four rectangles whose overlapping corners would composite twice.
    assert!(stream.contains("\nS\n"), "{stream}");
    assert_eq!(stream.matches(" re\n").count(), 2, "{stream}");
    let raw = String::from_utf8_lossy(&bytes);
    assert!(raw.contains("/G8080") && raw.contains("/GFFFF") && raw.contains("/ca 0.5019608"));
}

#[test]
fn given_an_opaque_document_when_rendered_then_declares_no_graphics_states() {
    let commands = [
        Command::BoxStart {
            size: crate::BoxSize::AUTO,
            style: decorated(1.0),
        },
        text("x"),
        Command::BoxEnd,
    ];
    let bytes = super::render(page(), &commands).unwrap();
    let raw = String::from_utf8_lossy(&bytes);
    assert!(!raw.contains("ExtGState") && !raw.contains(" gs\n"));
}

#[test]
fn given_a_rounded_box_that_does_not_fit_the_page_when_placed_then_moves_whole_to_the_next_page() {
    let style = super::BoxStyle {
        radius: round(4.0, 0),
        ..decorated(1.0)
    };
    let pdf = parsed(&[
        text("a"),
        text("b"),
        text("c"),
        Command::BoxStart {
            size: crate::BoxSize::AUTO,
            style,
        },
        text("x"),
        Command::BoxEnd,
    ]);
    let pages: Vec<String> = pdf
        .get_pages()
        .values()
        .map(|page| String::from_utf8(pdf.get_page_content(*page)).unwrap())
        .collect();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].matches(" c\n").count(), 0, "{}", pages[0]);
    assert_eq!(pages[1].matches(" c\n").count(), 8, "{}", pages[1]);
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
        orphans: 2,
        widows: 2,
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
    assert_eq!(
        operands(&pdf, "Tm"),
        [[1.0, 0.0, 0.25, 1.0, 10.0, 40.78369]]
    );
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
        orphans: 2,
        widows: 2,
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
    // 8pt remain after the spacer, below one 11.5pt line.
    let runs = [
        marked(2, super::Marker::Outside),
        run(4, false, 10.0, false),
    ];
    let pdf = parsed(&[
        Command::Spacer(Pt(32.0)),
        Command::Paragraph {
            orphans: 2,
            widows: 2,
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
    assert_eq!(shown(2).matches("40.78369 Td").count(), 2);
    assert!(shown(2).contains("\n1.65"), "{}", shown(2));
}

#[test]
fn given_an_inside_marker_when_laid_out_then_the_first_line_starts_after_it() {
    let runs = [marked(2, super::Marker::Inside), run(4, false, 10.0, false)];
    let pdf = parsed(&[Command::Paragraph {
        orphans: 2,
        widows: 2,
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
fn given_a_two_line_first_page_header_and_a_numbered_header_when_rendering_then_each_page_paints_and_reserves_its_own(
) {
    let mut renderer = super::Renderer::new(page());
    let plain = TextStyle::plain(Pt(8.0));
    renderer
        .set_furniture(false, false, line("Later \u{1e}", plain))
        .unwrap();
    let runs = vec![run(5, false, 8.0, true), run(4, false, 8.0, false)];
    let first =
        crate::OwnedCommand::Paragraph(TextAlign::Left, [2, 2], "Firstpage".into(), runs, vec![]);
    renderer.set_furniture(false, true, vec![first]).unwrap();
    renderer
        .push(&["a", "b", "c", "d", "e"].map(fixed_line))
        .unwrap();
    let pdf = lopdf::Document::load_mem(&renderer.finish().unwrap()).unwrap();
    // The 18.4pt first header leaves two 9.25pt lines; the 9.2pt later header leaves three.
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
    // 11.5pt plus 13.8pt passes half of the 40pt between the margins.
    assert_eq!(
        renderer.set_furniture(true, true, line("x", TextStyle::plain(Pt(12.0)))),
        Err(RenderError::PageOverflow)
    );
}

#[test]
fn given_a_page_number_whose_real_width_pulls_a_larger_word_up_a_line_when_finishing_then_rejects_with_page_overflow(
) {
    let page = Page::new(Pt(100.0), Pt(180.0), Edges::all(Pt(5.0))).unwrap();
    let mut renderer = super::Renderer::new(page);
    // In 90pt, five 20pt digits push the first 40pt "M" to a second line; "1" lets it
    // stay and pushes the second one down, so two 46pt lines replace 23pt and 46pt.
    let runs = vec![run(1, false, 20.0, false), run(4, false, 40.0, false)];
    let footer =
        crate::OwnedCommand::Paragraph(TextAlign::Left, [2, 2], "\u{1e} M M".into(), runs, vec![]);
    renderer.set_furniture(true, false, vec![footer]).unwrap();
    renderer.push(&[text("body")]).unwrap();
    assert_eq!(renderer.finish().err(), Some(RenderError::PageOverflow));
}

#[test]
fn given_space_between_and_center_when_a_row_lays_out_then_items_touch_both_edges_and_centre_across(
) {
    let style = flex(
        false,
        super::Justify::SpaceBetween,
        super::Align::Center,
        None,
    );
    let pdf = parsed(&[
        Command::FlexStart {
            style,
            items: &[ITEM; 2],
        },
        text("a"),
        Command::StackStart { gap: Pt(0.0) },
        text("b"),
        text("c"),
        Command::StackEnd,
        Command::FlexEnd,
    ]);
    // "b" is 5.56pt wide plus the 0.001pt that keeps it on one line; "a" sits half a line lower.
    let content = content(&pdf);
    for position in ["104.439 40.78369 Td", "10 35.03418 Td"] {
        assert!(content.contains(position), "{position}: {content}");
    }
}

#[test]
fn given_bases_wider_than_the_row_when_shrinking_then_an_item_stops_at_its_longest_word() {
    let basis = super::ImageSize::Fixed(Pt(80.0));
    let items = [super::FlexItem { basis, ..ITEM }; 2];
    let style = flex(false, super::Justify::Start, super::Align::Stretch, None);
    let pdf = parsed(&[
        Command::FlexStart {
            style,
            items: &items,
        },
        text("WWWWWWW"),
        text("a"),
        Command::FlexEnd,
    ]);
    // An even shrink leaves 50pt each, but seven W's need 66.08pt, so "a" takes the rest.
    assert!(
        content(&pdf).contains("76.08099 40.78369 Td"),
        "{}",
        content(&pdf)
    );
}

#[test]
fn given_grow_factors_and_bases_when_space_is_left_then_items_share_it_by_factor() {
    let basis = super::ImageSize::Fixed(Pt(20.0));
    let items = [1.0, 3.0].map(|grow| super::FlexItem {
        grow,
        basis,
        ..ITEM
    });
    let style = flex(false, super::Justify::Start, super::Align::Stretch, None);
    let pdf = parsed(&[
        Command::FlexStart {
            style,
            items: &items,
        },
        text("a"),
        text("b"),
        Command::FlexEnd,
    ]);
    // 60pt is left over: the first takes 15pt and ends at 35pt, the second takes 45pt.
    assert!(
        content(&pdf).contains("45 40.78369 Td"),
        "{}",
        content(&pdf)
    );
}

#[test]
fn given_stretch_when_a_painted_item_is_shorter_than_the_row_then_its_box_fills_the_row() {
    let style = flex(false, super::Justify::Start, super::Align::Stretch, None);
    let pdf = parsed(&[
        Command::FlexStart {
            style,
            items: &[ITEM; 2],
        },
        sized(super::BoxSize::AUTO, RED),
        text("a"),
        Command::BoxEnd,
        Command::StackStart { gap: Pt(0.0) },
        text("b"),
        text("c"),
        Command::StackEnd,
        Command::FlexEnd,
    ]);
    assert!(
        content(&pdf).contains("10 27.001953 5.561 22.998047 re"),
        "{}",
        content(&pdf)
    );
}

#[test]
fn given_a_max_width_and_auto_side_margins_when_a_box_lays_out_then_it_centres() {
    let size = super::BoxSize {
        max_width: super::ImageSize::Fixed(Pt(60.0)),
        auto_margin: 2 | 8,
        ..super::BoxSize::AUTO
    };
    let pdf = parsed(&[sized(size, RED), text("a"), Command::BoxEnd]);
    assert!(
        content(&pdf).contains("30 38.500977 60 11.499023 re"),
        "{}",
        content(&pdf)
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
            orphans: 2,
            widows: 2,
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
                orphans: 2,
                widows: 2,
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
            orphans: 2,
            widows: 2,
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
        orphans: 2,
        widows: 2,
        links: &[],
        align: TextAlign::Left,
        text: "ab",
        runs: &runs,
    }]);
    let baseline = operands(&pdf, "Td")[0][1];
    // The browser metrics give an 11.5pt box over the two 5.56pt glyphs.
    let (corner, edges) = (&operands(&pdf, "m")[0], operands(&pdf, "l"));
    assert!(
        (corner[1] - (baseline - 2.2827)).abs() < 0.001,
        "{corner:?}"
    );
    assert!((edges[0][0] - corner[0] - 11.12).abs() < 0.001, "{edges:?}");
    assert!(
        (edges[1][1] - corner[1] - 11.499).abs() < 0.001,
        "{edges:?}"
    );
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
        orphans: 2,
        widows: 2,
        links: &[],
        align: TextAlign::Left,
        text: "AV",
        runs: &runs,
    }]);
    // The V segment starts at A's 6.67pt advance less the 0.7pt kern.
    assert!((operands(&pdf, "Td")[1][0] - 5.97).abs() < 0.001);
}

#[test]
fn given_border_box_sizing_when_a_box_has_padding_then_its_width_includes_the_padding() {
    let size = super::BoxSize {
        width: super::ImageSize::Fixed(Pt(50.0)),
        border_box: true,
        ..super::BoxSize::AUTO
    };
    let Command::BoxStart { style, .. } = sized(size, RED) else {
        unreachable!()
    };
    let style = super::BoxStyle {
        padding: Edges::all(Pt(5.0)),
        ..style
    };
    let pdf = parsed(&[
        Command::BoxStart { style, size },
        text("a"),
        Command::BoxEnd,
    ]);
    assert!(
        content(&pdf).contains("10 28.500977 50 21.499023 re"),
        "{}",
        content(&pdf)
    );
}

#[test]
fn given_content_taller_than_a_fixed_height_when_laid_out_then_rejects_with_content_overflow() {
    let size = super::BoxSize {
        height: Some(Pt(5.0)),
        ..super::BoxSize::AUTO
    };
    assert_eq!(
        super::render(page(), &[sized(size, None), text("a"), Command::BoxEnd]),
        Err(RenderError::ContentOverflow)
    );
}

#[test]
fn given_a_fixed_height_box_below_the_fold_when_placed_then_it_moves_whole_to_the_next_page() {
    let size = super::BoxSize {
        height: Some(Pt(35.0)),
        ..super::BoxSize::AUTO
    };
    let pdf = parsed(&[
        text("first"),
        sized(size, None),
        text("a"),
        text("b"),
        Command::BoxEnd,
    ]);
    assert_eq!(pdf.extract_text(&[1]).unwrap().trim(), "first");
    assert_eq!(pdf.extract_text(&[2]).unwrap().trim(), "a\nb");
}

#[test]
fn given_a_column_with_a_height_and_space_between_when_laid_out_then_items_touch_top_and_bottom() {
    let style = flex(
        true,
        super::Justify::SpaceBetween,
        super::Align::Stretch,
        Some(40.0),
    );
    let pdf = parsed(&[
        Command::FlexStart {
            style,
            items: &[ITEM; 2],
        },
        text("a"),
        text("b"),
        Command::FlexEnd,
    ]);
    // The last line box ends on the 10pt bottom margin, with a 2.282715pt descent.
    let content = content(&pdf);
    for position in ["10 40.78369 Td", "10 12.282715 Td"] {
        assert!(content.contains(position), "{position}: {content}");
    }
}

fn flex(
    column: bool,
    justify: super::Justify,
    align: super::Align,
    height: Option<f32>,
) -> super::Flex {
    super::Flex {
        column,
        justify,
        align,
        gap: Pt(0.0),
        min_height: Pt(height.unwrap_or(0.0)),
        max_height: height.map(Pt),
    }
}

const ITEM: super::FlexItem = super::FlexItem {
    grow: 0.0,
    shrink: 1.0,
    basis: super::ImageSize::Auto,
    align: None,
};

fn sized(size: super::BoxSize, background: Option<Rgb>) -> Command<'static> {
    Command::BoxStart {
        style: super::BoxStyle {
            margin: Edges::all(Pt(0.0)),
            padding: Edges::all(Pt(0.0)),
            border: Edges::all(Pt(0.0)),
            background,
            border_color: Edges::all(Rgb::BLACK),
            border_style: Edges::all(LineStyle::Solid),
            radius: None,
            alpha: OPAQUE,
        },
        size,
    }
}

const RED: Option<Rgb> = Some(Rgb { r: 255, g: 0, b: 0 });

#[cfg(feature = "parallel")]
#[test]
fn given_many_pages_and_two_fonts_when_finished_on_one_or_four_threads_then_the_bytes_match() {
    let abel = include_bytes!("../../../packages/flashpdf/test/fixtures/Abel-Regular.ttf");
    let paragraph = (1..=400)
        .map(|n| format!("Line {n} \u{20ac}"))
        .collect::<Vec<_>>()
        .join(" ");
    let render = || {
        let fonts = (0..2)
            .map(|_| super::EmbeddedFont::parse(abel.to_vec()).unwrap())
            .collect();
        let mut renderer = super::Renderer::with_fonts(page(), fonts);
        for slot in [12, 13] {
            let style = TextStyle {
                font: super::FontId::new(slot),
                ..TextStyle::plain(Pt(10.0))
            };
            renderer
                .push(&[Command::Text {
                    text: &paragraph,
                    style,
                }])
                .unwrap();
        }
        renderer.finish().unwrap()
    };
    let on = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(render)
    };
    assert_eq!(on(1), on(4));
}

/// Abel with one more table appended, its record sorted into a directory grown by one.
fn abel_with(tag: &[u8; 4], table: &[u16]) -> Vec<u8> {
    let count = usize::from(u16::from_be_bytes([ABEL[4], ABEL[5]]));
    let data: Vec<u8> = table.iter().flat_map(|word| word.to_be_bytes()).collect();
    let mut records: Vec<Vec<u8>> = ABEL[12..12 + count * 16]
        .chunks(16)
        .map(|record| {
            let offset = u32::from_be_bytes(record[8..12].try_into().unwrap()) + 16;
            [&record[..8], &offset.to_be_bytes(), &record[12..]].concat()
        })
        .collect();
    let at = (ABEL.len() as u32 + 16).to_be_bytes();
    records.push([&tag[..], &[0; 4], &at, &(data.len() as u32).to_be_bytes()].concat());
    records.sort();
    let mut font = ABEL[..12].to_vec();
    font[4..6].copy_from_slice(&(count as u16 + 1).to_be_bytes());
    font.extend(records.concat());
    font.extend(&ABEL[12 + count * 16..]);
    font.extend(data);
    font
}

/// A GSUB or GPOS table whose DFLT script turns `feature` on, with one lookup of `kind`.
fn layout_table(feature: &[u8; 4], kind: u16, subtable: &[u16]) -> Vec<u16> {
    let half = |at: usize| u16::from_be_bytes([feature[at], feature[at + 1]]);
    let mut words = vec![1, 0, 10, 30, 44];
    words.extend([1, u16::from_be_bytes(*b"DF"), u16::from_be_bytes(*b"LT")]);
    words.extend([8, 4, 0, 0, 0xffff, 1, 0]);
    words.extend([1, half(0), half(2), 8, 0, 1, 0]);
    words.extend([1, 4, kind, 0, 1, 8]);
    words.extend(subtable);
    words
}

fn abel_glyph(character: char) -> u16 {
    super::EmbeddedFont::parse(ABEL.to_vec())
        .unwrap()
        .glyph(character)
        .unwrap()
}

/// Abel whose `liga` turns f i into the glyph Abel draws for Æ.
fn ligating_abel() -> Vec<u8> {
    let [f, i, ae] = ['f', 'i', '\u{c6}'].map(abel_glyph);
    abel_with(
        b"GSUB",
        &layout_table(b"liga", 4, &[1, 8, 1, 14, 1, 1, f, 1, 4, ae, 2, i]),
    )
}

fn render_with(font: Vec<u8>, text: &str, typography: super::Typography, tagged: bool) -> Vec<u8> {
    let font = super::EmbeddedFont::parse(font).unwrap();
    let mut renderer = super::Renderer::with_fonts(Page::A4, vec![font]);
    renderer.set_metadata(crate::pdf::Metadata {
        language: "en".into(),
        tagged,
        ..Default::default()
    });
    renderer
        .push(&[Command::Text {
            text,
            style: TextStyle {
                font: super::FontId::new(12),
                typography,
                ..TextStyle::plain(Pt(12.0))
            },
        }])
        .unwrap();
    renderer.finish().unwrap()
}

/// The show strings of the first page as glyph ids.
fn shown_glyphs(pdf: &lopdf::Document) -> Vec<u16> {
    operations(pdf)
        .iter()
        .filter(|operation| matches!(operation.operator.as_str(), "Tj" | "TJ"))
        .flat_map(|operation| match &operation.operands[0] {
            lopdf::Object::Array(items) => items.clone(),
            other => vec![other.clone()],
        })
        .filter_map(|item| item.as_str().ok().map(<[u8]>::to_vec))
        .flat_map(|bytes| {
            bytes
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn given_a_font_whose_liga_joins_f_and_i_when_rendered_then_shows_the_ligature_maps_it_to_both_letters_and_embeds_its_outline(
) {
    let bytes = render_with(ligating_abel(), "fit", super::Typography::NORMAL, false);
    assert_eq!(
        bytes,
        render_with(ligating_abel(), "fit", super::Typography::NORMAL, false)
    );
    let pdf = lopdf::Document::load_mem(&bytes).unwrap();
    let [t, ae] = ['t', '\u{c6}'].map(abel_glyph);
    assert_eq!(shown_glyphs(&pdf), [ae, t]);
    assert_eq!(page_text(&pdf, 1), "fit");
    // The program keeps the ligature's outline: the subset of Æ and t alone is as large.
    let plain = lopdf::Document::load_mem(&abel_text("\u{c6}t").unwrap()).unwrap();
    let program = |pdf: &lopdf::Document| {
        pdf.objects
            .values()
            .filter_map(|object| object.as_stream().ok())
            .find(|stream| stream.dict.get(b"Length1").is_ok())
            .unwrap()
            .decompressed_content()
            .unwrap()
            .len()
    };
    assert_eq!(program(&pdf), program(&plain));
}

#[test]
fn given_letter_spacing_when_shaping_then_skips_the_optional_ligature() {
    let spaced = super::Typography {
        letter_spacing: 1.0,
        ..super::Typography::NORMAL
    };
    let pdf =
        lopdf::Document::load_mem(&render_with(ligating_abel(), "fi", spaced, false)).unwrap();
    assert_eq!(shown_glyphs(&pdf), ['f', 'i'].map(abel_glyph));
}

#[test]
fn given_abels_kern_table_when_shaped_then_harfrust_kerns_av_unless_kerning_is_off() {
    let kerned = |kerning| {
        let typography = super::Typography {
            kerning,
            ..super::Typography::NORMAL
        };
        let pdf = lopdf::Document::load_mem(&render_with(ABEL.to_vec(), "AVA", typography, false))
            .unwrap();
        operands(&pdf, "TJ")
    };
    // -41 of 2048 units is -20 thousandths, a TJ adjustment of 20 before each glyph after A V.
    assert_eq!(kerned(true), [[20.0, 20.0]]);
    assert!(kerned(false).is_empty());
}

#[test]
fn given_a_gpos_placement_that_raises_a_glyph_when_rendered_then_lifts_only_that_glyph_with_a_text_rise(
) {
    let i = abel_glyph('i');
    // 205 of 2048 units up is 100 thousandths, 1.2pt at 12pt.
    let font = abel_with(b"GPOS", &layout_table(b"mark", 1, &[1, 8, 2, 205, 1, 1, i]));
    let pdf =
        lopdf::Document::load_mem(&render_with(font, "xix", super::Typography::NORMAL, false))
            .unwrap();
    assert_eq!(operands(&pdf, "Ts"), [[1.2], [0.0]]);
    assert_eq!(shown_glyphs(&pdf), ['x', 'i', 'x'].map(abel_glyph));
    assert_eq!(page_text(&pdf, 1), "xix");
}

#[test]
fn given_a_character_shaped_into_two_glyphs_when_tagged_then_a_span_carries_its_actual_text() {
    let [x, a, b] = ['x', 'a', 'b'].map(abel_glyph);
    let font = abel_with(
        b"GSUB",
        &layout_table(b"ccmp", 2, &[1, 8, 1, 14, 1, 1, x, 2, a, b]),
    );
    let tagged = |tagged| {
        let pdf = lopdf::Document::load_mem(&render_with(
            font.clone(),
            "axb",
            super::Typography::NORMAL,
            tagged,
        ))
        .unwrap();
        (shown_glyphs(&pdf), content(&pdf))
    };
    let (glyphs, shown) = tagged(true);
    assert_eq!(glyphs, [a, a, b, b]);
    assert!(
        shown.contains("/Span <<\n  /ActualText (x)\n>> BDC"),
        "{shown}"
    );
    assert!(!tagged(false).1.contains("ActualText"));
}

fn visual_glyphs(text: &str) -> Vec<u16> {
    let bytes = render_with(ABEL.to_vec(), text, super::Typography::NORMAL, false);
    shown_glyphs(&lopdf::Document::load_mem(&bytes).unwrap())
}

#[test]
fn given_a_right_to_left_override_when_laid_out_then_its_words_and_their_letters_read_right_to_left(
) {
    assert_eq!(
        visual_glyphs("ab \u{202e}cd ef\u{202c}"),
        ['a', 'b', ' ', 'f', 'e', ' ', 'd', 'c'].map(abel_glyph)
    );
}

#[test]
fn given_brackets_in_right_to_left_text_when_shaped_then_they_mirror() {
    // Reversed and mirrored, `(a)` reads `(a)` again; unmirrored it would read `)a(`.
    assert_eq!(
        visual_glyphs("\u{202e}(a)\u{202c}"),
        ['(', 'a', ')'].map(abel_glyph)
    );
}

#[test]
fn given_a_paragraph_whose_first_strong_character_is_right_to_left_when_laid_out_then_trailing_punctuation_moves_to_the_left(
) {
    assert_eq!(
        visual_glyphs("\u{200f}ab cd!"),
        ['!', 'a', 'b', ' ', 'c', 'd'].map(abel_glyph)
    );
}

#[test]
fn given_right_to_left_words_that_wrap_when_laid_out_then_each_line_reverses_only_its_own_words() {
    let words = "\u{202e}one two three four five six seven eight nine ten\u{202c}";
    let pdf = lopdf::Document::load_mem(&render_with(
        ABEL.to_vec(),
        &words.repeat(8),
        super::Typography::NORMAL,
        false,
    ))
    .unwrap();
    let text = page_text(&pdf, 1);
    let first = text.lines().next().unwrap();
    // The first line holds the first words, mirrored, each read backwards.
    assert!(first.ends_with("owt eno"), "{first}");
    assert!(text.lines().count() > 1);
}

const ARABIC: &[u8] =
    include_bytes!("../../../packages/flashpdf/test/fixtures/NotoSansArabic-subset.ttf");
const HEBREW: &[u8] =
    include_bytes!("../../../packages/flashpdf/test/fixtures/NotoSansHebrew-subset.ttf");

fn nominal(font: &[u8], character: char) -> u16 {
    super::EmbeddedFont::parse(font.to_vec())
        .unwrap()
        .glyph(character)
        .unwrap()
}

#[test]
fn given_hebrew_when_laid_out_then_its_letters_show_right_to_left() {
    let pdf = lopdf::Document::load_mem(&render_with(
        HEBREW.to_vec(),
        "\u{5e9}\u{5dc}\u{5d5}\u{5dd}",
        super::Typography::NORMAL,
        false,
    ))
    .unwrap();
    let logical =
        ['\u{5e9}', '\u{5dc}', '\u{5d5}', '\u{5dd}'].map(|letter| nominal(HEBREW, letter));
    let mut visual = logical;
    visual.reverse();
    assert_eq!(shown_glyphs(&pdf), visual);
}

#[test]
fn given_arabic_when_shaped_then_its_letters_join_read_right_to_left_and_copy_back_as_letters() {
    let word = "\u{633}\u{644}\u{627}\u{645}";
    let pdf = lopdf::Document::load_mem(&render_with(
        ARABIC.to_vec(),
        word,
        super::Typography::NORMAL,
        false,
    ))
    .unwrap();
    let glyphs = shown_glyphs(&pdf);
    // Left to right on the page: meem alone after alef, final alef, medial lam, initial seen.
    assert_eq!(glyphs.len(), 4);
    assert_eq!(glyphs[0], nominal(ARABIC, '\u{645}'));
    assert_ne!(glyphs[2], nominal(ARABIC, '\u{644}'));
    assert_ne!(glyphs[3], nominal(ARABIC, '\u{633}'));
    assert_eq!(page_text(&pdf, 1).chars().rev().collect::<String>(), word);
}
