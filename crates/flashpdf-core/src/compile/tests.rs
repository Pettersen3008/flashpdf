use super::*;

/// Collects records; drawings take slots from 100.
#[derive(Default)]
struct Collect {
    records: Vec<Record>,
    images: Vec<Vec<u8>>,
}

impl Sink for Collect {
    fn record(&mut self, record: &Record, _: &[u8]) -> std::result::Result<(), String> {
        self.records.push(record.clone());
        Ok(())
    }
    fn add_image(&mut self, bytes: Vec<u8>) -> std::result::Result<u16, String> {
        self.images.push(bytes);
        Ok(99 + self.images.len() as u16)
    }
    fn add_raster(&mut self, handle: u16) -> std::result::Result<u16, String> {
        Ok(handle)
    }
}

fn text(value: &str) -> Node {
    Node::Text(value.into())
}

fn el(tag: &str, style: &str, children: Vec<Node>) -> Element {
    Element {
        tag: tag.into(),
        style: if style.is_empty() {
            InlineStyle::None
        } else {
            InlineStyle::Text(style.into())
        },
        children,
        ..Element::default()
    }
}

fn node(tag: &str, style: &str, children: Vec<Node>) -> Node {
    Node::Element(el(tag, style, children))
}

fn with(element: Element, change: impl FnOnce(&mut Element)) -> Node {
    let mut element = element;
    change(&mut element);
    Node::Element(element)
}

fn helvetica() -> Fonts {
    HashMap::from([("Helvetica".into(), HELVETICA)])
}

fn compiled_with(nodes: Vec<Node>, fonts: &Fonts, outline: bool) -> Result<Collect> {
    let styler = Styler::new(&[], 523.0).unwrap();
    let mut compiler = Compiler::new(styler, fonts.clone(), Collect::default());
    compiler.body(&nodes, outline)?;
    Ok(compiler.writer.sink)
}

fn records(nodes: Vec<Node>) -> Vec<Record> {
    compiled_with(nodes, &helvetica(), false).unwrap().records
}

fn rejects(nodes: Vec<Node>) -> String {
    match compiled_with(nodes, &helvetica(), false) {
        Ok(_) => panic!("compiled"),
        Err(error) => error.into_message(),
    }
}

/// compile.unit.test.mjs `run`: a 12pt black run in a font slot.
fn r(font: u32, value: &str) -> Run {
    Run {
        font,
        size: 12.0,
        line_height: 0.0,
        color: [0, 0, 0],
        text: value.into(),
        hard_break: false,
        underline: false,
        line_through: false,
        decoration_color: None,
        link: None,
        opacity: 1.0,
        oblique: false,
        letter_spacing: 0.0,
        word_spacing: 0.0,
        kerning: true,
        rise: 0.0,
        background: None,
        marker: 0,
        white_space: WhiteSpace::Normal,
        transform: Transform::None,
    }
}

fn para(runs: Vec<Run>) -> Record {
    Record::Paragraph {
        runs,
        align: 0,
        orphans: 2.0,
        widows: 2.0,
    }
}

fn names(records: &[Record]) -> Vec<&'static str> {
    records
        .iter()
        .map(|record| match record {
            Record::Spacer(_) => "spacer",
            Record::StackStart(_) => "stackStart",
            Record::StackEnd => "stackEnd",
            Record::RowStart(..) => "rowStart",
            Record::RowEnd => "rowEnd",
            Record::FlexStart(..) => "flexStart",
            Record::FlexEnd => "flexEnd",
            Record::PageBreak => "pageBreak",
            Record::Repeat { .. } => "repeat",
            Record::Paragraph { .. } => "paragraph",
            Record::TableStart(..) => "tableStart",
            Record::TableEnd => "tableEnd",
            Record::Image { .. } => "image",
            Record::BoxStart(_) => "boxStart",
            Record::BoxEnd => "boxEnd",
            Record::Anchor { .. } => "anchor",
            Record::Avoid => "avoid",
            Record::End => "end",
        })
        .collect()
}

/// The runs of every paragraph, in order.
fn paragraphs(records: &[Record]) -> Vec<Vec<Run>> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::Paragraph { runs, .. } => Some(runs.clone()),
            _ => None,
        })
        .collect()
}

fn boxes(records: &[Record]) -> Vec<Box> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::BoxStart(value) => Some(*value),
            _ => None,
        })
        .collect()
}

const TOP: Slot = Slot::Cell { align: 0, rows: 1 };
const SHARE: Column = Column {
    kind: 1,
    value: 1.0,
};

fn fixed_column(value: f64) -> Column {
    Column { kind: 0, value }
}

/// A cell's content: a stack around one paragraph, as a non-collapsing block in a row.
fn cell_records(paragraph: Record) -> Vec<Record> {
    vec![Record::StackStart(0.0), paragraph, Record::StackEnd]
}

#[test]
fn given_text_with_an_inline_bold_child_when_compiling_then_emits_one_paragraph_with_two_runs() {
    let records = records(vec![node(
        "p",
        "",
        vec![text("Hello "), node("b", "", vec![text("World")])],
    )]);
    assert_eq!(
        records,
        [
            Record::Spacer(12.0),
            para(vec![r(0, "Hello "), r(1, "World")])
        ]
    );
}

#[test]
fn given_a_br_between_text_when_compiling_then_flags_a_hard_break_on_the_preceding_run() {
    let records = records(vec![node(
        "div",
        "",
        vec![
            text("one"),
            node("br", "", vec![]),
            node("br", "", vec![]),
            text("two"),
        ],
    )]);
    let broken = |value| Run {
        hard_break: true,
        ..r(0, value)
    };
    assert_eq!(
        records,
        [para(vec![broken("one"), broken(""), r(0, "two")])]
    );
}

#[test]
fn given_a_padded_span_in_a_block_when_compiling_then_it_stays_a_box_beside_the_paragraph() {
    let records = records(vec![node(
        "div",
        "",
        vec![
            text("Total"),
            node("span", "padding: 2px; font-weight: bold", vec![text("$1")]),
        ],
    )]);
    assert_eq!(
        names(&records),
        ["paragraph", "boxStart", "paragraph", "boxEnd"]
    );
    assert_eq!(records[2], para(vec![r(1, "$1")]));
}

#[test]
fn given_a_padded_span_inside_a_paragraph_when_compiling_then_rejects_with_the_element_path() {
    let message = rejects(vec![node(
        "p",
        "",
        vec![node("span", "padding: 2px", vec![text("x")])],
    )]);
    assert_eq!(message, format!("<p> {INLINE_ONLY} on <p>"));
}

#[test]
fn given_a_link_inside_a_paragraph_when_compiling_then_its_runs_carry_the_href_blue_and_underline()
{
    let link = with(el("a", "", vec![text("the portal")]), |a| {
        a.href = Some("https://example.com/a%20b".into());
    });
    let records = records(vec![node(
        "p",
        "",
        vec![
            text("See "),
            link,
            node("span", "text-decoration: line-through", vec![text(" old")]),
        ],
    )]);
    let linked = Run {
        color: [0, 0, 238],
        underline: true,
        link: Some("https://example.com/a%20b".into()),
        ..r(0, "the portal")
    };
    let struck = Run {
        line_through: true,
        ..r(0, " old")
    };
    assert_eq!(
        records,
        [
            Record::Spacer(12.0),
            para(vec![r(0, "See "), linked, struck])
        ]
    );
}

#[test]
fn given_a_link_around_an_image_when_compiling_then_emits_the_image_record_with_the_href_and_alt() {
    let image = with(el("img", "width: 50%; height: 16px", vec![]), |img| {
        img.picture = Some(Picture::Image(7));
        img.alt = Some("Logo".into());
    });
    let link = with(el("a", "", vec![image]), |a| {
        a.href = Some("mailto:billing@example.com".into());
    });
    assert_eq!(
        records(vec![link]),
        [Record::Image {
            slot: 7,
            width: Some(Dimension {
                kind: 2,
                value: 50.0
            }),
            height: Some(12.0),
            max_width: None,
            max_height: None,
            link: Some("mailto:billing@example.com".into()),
            alt: "Logo".into(),
            opacity: 1.0,
        }]
    );
}

#[test]
fn given_a_table_with_thead_and_tbody_when_compiling_then_emits_the_container_with_header_count_1_and_one_row_per_tr(
) {
    let cell = |tag: &str, style: &str, value: &str| node(tag, style, vec![text(value)]);
    let records = records(vec![node(
        "table",
        "",
        vec![
            node(
                "thead",
                "",
                vec![node(
                    "tr",
                    "",
                    vec![cell("th", "", "Item"), cell("th", "width: 80pt", "Amount")],
                )],
            ),
            node(
                "tbody",
                "",
                vec![node(
                    "tr",
                    "",
                    vec![cell("td", "", "Coffee"), cell("td", "", "$4")],
                )],
            ),
        ],
    )]);
    let columns = vec![SHARE, fixed_column(80.0)];
    let row = |font, values: [&str; 2]| {
        let mut row = vec![Record::RowStart(columns.clone(), vec![TOP, TOP])];
        for value in values {
            row.extend(cell_records(Record::Paragraph {
                runs: vec![r(font, value)],
                align: if font == 1 { 1 } else { 0 },
                orphans: 2.0,
                widows: 2.0,
            }));
        }
        row.push(Record::RowEnd);
        row
    };
    let mut expected = vec![Record::TableStart(1, 0, SHARE)];
    expected.extend(row(1, ["Item", "Amount"]));
    expected.extend(row(0, ["Coffee", "$4"]));
    expected.push(Record::TableEnd);
    assert_eq!(records, expected);
}

#[test]
fn given_rows_with_different_cell_counts_when_compiling_then_rejects_naming_the_tr_path() {
    let short = with(el("tr", "", vec![node("td", "", vec![text("a")])]), |tr| {
        tr.classes = vec!["short".into()];
    });
    let message = rejects(vec![node(
        "table",
        "",
        vec![
            node(
                "tr",
                "",
                vec![
                    node("td", "", vec![text("a")]),
                    node("td", "", vec![text("b")]),
                ],
            ),
            short,
        ],
    )]);
    assert_eq!(
        message,
        "<tr> has 1 cells but the table has 2 columns on <tr.short> in <table>"
    );
}

fn td(style: &str, change: impl FnOnce(&mut Element)) -> Node {
    with(el("td", style, vec![text("x")]), change)
}

fn tr(cells: Vec<Node>) -> Node {
    node("tr", "", cells)
}

#[test]
fn given_a_totals_row_whose_label_spans_three_columns_when_compiling_then_each_column_takes_its_width_from_its_first_cell_without_col_span(
) {
    let span = |count| move |cell: &mut Element| cell.col_span = Some(count);
    let records = records(vec![node(
        "table",
        "",
        vec![
            tr(vec![
                td("", span(2)),
                td("width: 40pt", |_| {}),
                td("width: 60pt", |_| {}),
            ]),
            tr(vec![
                td("width: 50%", |_| {}),
                td("", |_| {}),
                td("width: 90pt", |_| {}),
                td("", |_| {}),
            ]),
            tr(vec![td("", span(3)), td("", |_| {})]),
        ],
    )]);
    let columns = vec![
        Column {
            kind: 2,
            value: 50.0,
        },
        SHARE,
        fixed_column(40.0),
        fixed_column(60.0),
    ];
    let rows: Vec<Record> = records
        .into_iter()
        .filter(|record| matches!(record, Record::RowStart(..)))
        .collect();
    assert_eq!(
        rows,
        [
            vec![TOP, Slot::Join, TOP, TOP],
            vec![TOP, TOP, TOP, TOP],
            vec![TOP, Slot::Join, Slot::Join, TOP],
        ]
        .map(|slots| Record::RowStart(columns.clone(), slots))
    );
}

#[test]
fn given_a_cell_spanning_two_rows_beside_striped_rows_when_compiling_then_both_rows_sit_in_one_stack_and_each_rows_background_moves_into_its_own_cells(
) {
    let records = records(vec![node(
        "table",
        "",
        vec![node(
            "tbody",
            "",
            vec![
                node(
                    "tr",
                    "background: #ff0000",
                    vec![
                        with(el("td", "", vec![text("a")]), |cell| {
                            cell.row_span = Some(2)
                        }),
                        node("td", "vertical-align: bottom", vec![text("b")]),
                    ],
                ),
                node(
                    "tr",
                    "background: #0000ff; vertical-align: middle",
                    vec![node("td", "", vec![text("c")])],
                ),
            ],
        )],
    )]);
    let filled = |background| {
        Record::BoxStart(Box {
            background: Some(background),
            ..EMPTY
        })
    };
    let cell = |background, value| {
        let mut cell = vec![filled(background)];
        cell.extend(cell_records(para(vec![r(0, value)])));
        cell.push(Record::BoxEnd);
        cell
    };
    let mut expected = vec![
        Record::TableStart(0, 0, SHARE),
        Record::StackStart(0.0),
        Record::RowStart(
            vec![SHARE, SHARE],
            vec![
                Slot::Cell { align: 0, rows: 2 },
                Slot::Cell { align: 2, rows: 1 },
            ],
        ),
    ];
    expected.extend(cell([255, 0, 0], "a"));
    expected.extend(cell([255, 0, 0], "b"));
    expected.push(Record::RowEnd);
    expected.push(Record::RowStart(
        vec![SHARE, SHARE],
        vec![Slot::Covered, Slot::Cell { align: 1, rows: 1 }],
    ));
    expected.extend(cell([0, 0, 255], "c"));
    expected.extend([Record::RowEnd, Record::StackEnd, Record::TableEnd]);
    assert_eq!(records, expected);
}

#[test]
fn given_collapsed_1pt_cell_borders_under_a_2pt_header_rule_when_compiling_then_each_cell_draws_half_of_every_shared_edge_and_all_of_each_outer_edge(
) {
    let grey = "border: 1pt solid #808080";
    let records = records(vec![node(
        "table",
        "border-collapse: collapse",
        vec![
            node(
                "thead",
                "",
                vec![node(
                    "tr",
                    "border-bottom: 2pt solid black",
                    vec![
                        node("th", grey, vec![text("a")]),
                        node("th", grey, vec![text("b")]),
                    ],
                )],
            ),
            node(
                "tbody",
                "",
                vec![tr(vec![
                    node("td", grey, vec![text("c")]),
                    node("td", grey, vec![text("d")]),
                ])],
            ),
        ],
    )]);
    let (black, gray) = ([0, 0, 0], [128, 128, 128]);
    // The row's rule merges into its cells, so the row writes no box of its own.
    let edges: Vec<_> = boxes(&records)
        .iter()
        .map(|value| (value.border, value.border_color))
        .collect();
    assert_eq!(
        edges,
        [
            ([1.0, 0.5, 1.0, 1.0], [gray, gray, black, gray]),
            ([1.0, 1.0, 1.0, 0.5], [gray, gray, black, gray]),
            ([1.0, 0.5, 1.0, 1.0], [black, gray, gray, gray]),
            ([1.0, 1.0, 1.0, 0.5], [black, gray, gray, gray]),
        ]
    );
}

#[test]
fn given_table_input_outside_the_supported_subset_when_compiling_then_rejects_with_the_reason() {
    let table = |rows| node("table", "", rows);
    let cases = [
        (
            table(vec![node(
                "tbody",
                "",
                vec![tr(vec![td("", |cell| cell.row_span = Some(2))])],
            )]),
            "rowSpan 2 runs past the last row of its <tbody>; rows joined by rowSpan stay in one row group on <td> in <tr> > <tbody> > <table>",
        ),
        (
            table(vec![
                tr(vec![td("", |_| {}), td("", |cell| cell.row_span = Some(2))]),
                tr(vec![td("", |cell| cell.col_span = Some(2))]),
            ]),
            "colSpan runs into a cell that rowSpan brings down from an earlier row on <td> in <tr> > <table>",
        ),
        (
            table(vec![tr(vec![td("width: 50pt", |cell| cell.col_span = Some(2))])]),
            "width and flex size one column; set them on a cell without colSpan in that column on <td> in <tr> > <table>",
        ),
        (
            table(vec![tr(vec![td("vertical-align: baseline", |_| {})])]),
            "vertical-align: baseline is not implemented in tables; use top, middle, or bottom on <td> in <tr> > <table>",
        ),
        (
            node("table", "border-spacing: 2pt", vec![tr(vec![td("", |_| {})])]),
            "border-spacing is not implemented: cells tile their row; use cell padding, or border-collapse: collapse for single rules on <table>",
        ),
        (
            node("div", "vertical-align: middle", vec![text("x")]),
            "vertical-align applies only to inline elements and <td>, <th>, <tr>, and row groups on <div>",
        ),
    ];
    for (input, message) in cases {
        assert_eq!(rejects(vec![input]), message);
    }
}

#[test]
fn given_a_td_outside_a_table_row_when_compiling_then_rejects_naming_it() {
    assert_eq!(
        rejects(vec![node("div", "", vec![node("td", "", vec![text("x")])])]),
        "<td> belongs in a <tr> inside a <table> on <td> in <div>"
    );
}

#[test]
fn given_an_hr_with_break_before_page_when_compiling_then_breaks_the_page_before_the_rule() {
    let records = records(vec![node("hr", "break-before: page", vec![])]);
    assert_eq!(
        names(&records),
        ["pageBreak", "spacer", "boxStart", "boxEnd"]
    );
}

#[test]
fn given_orphans_and_widows_on_a_block_when_compiling_then_its_paragraphs_inherit_them() {
    let records = records(vec![node(
        "div",
        "orphans: 3; widows: 1",
        vec![node("p", "", vec![text("x")])],
    )]);
    let found: Vec<_> = records
        .into_iter()
        .filter(|record| matches!(record, Record::Paragraph { .. }))
        .collect();
    assert_eq!(
        found,
        [Record::Paragraph {
            runs: vec![r(0, "x")],
            align: 0,
            orphans: 3.0,
            widows: 1.0
        }]
    );
}

#[test]
fn given_break_inside_avoid_on_a_paragraph_when_compiling_then_it_is_one_kept_together_group() {
    let records = records(vec![node("p", "break-inside: avoid-page", vec![text("x")])]);
    assert_eq!(
        names(&records),
        ["spacer", "stackStart", "paragraph", "stackEnd"]
    );
}

#[test]
fn given_width_on_an_hr_or_padding_on_a_br_when_compiling_then_honours_the_rule_width_and_rejects_br_padding(
) {
    let records = records(vec![node("hr", "width: 50%", vec![])]);
    let size = boxes(&records)[0].size.unwrap();
    assert_eq!(size.width.unwrap().value, 50.0);
    assert_eq!(
        rejects(vec![node(
            "p",
            "",
            vec![text("a"), node("br", "padding: 10px", vec![]), text("b")],
        )]),
        "<br> takes no paddingTop; style the text around it on <br> in <p>"
    );
}

#[test]
fn given_a_flex_row_with_a_12pt_gap_when_compiling_then_a_fixed_12pt_empty_column_separates_its_items(
) {
    let records = records(vec![node(
        "div",
        "display: flex; gap: 12pt",
        vec![
            node("div", "", vec![text("a")]),
            node("div", "", vec![text("b")]),
        ],
    )]);
    let mut expected = vec![Record::RowStart(
        vec![SHARE, fixed_column(12.0), SHARE],
        vec![],
    )];
    expected.extend(cell_records(para(vec![r(0, "a")])));
    expected.push(Record::Spacer(0.0));
    expected.extend(cell_records(para(vec![r(0, "b")])));
    expected.push(Record::RowEnd);
    assert_eq!(records, expected);
}

#[test]
fn given_whitespace_only_text_between_flex_items_and_column_children_when_compiling_then_it_adds_no_column_gap_or_line(
) {
    let records = records(vec![node(
        "main",
        "display: flex; flex-direction: column; gap: 10pt",
        vec![
            node(
                "div",
                "display: flex",
                vec![
                    node("span", "", vec![text("a")]),
                    text(" "),
                    node("span", "", vec![text("b")]),
                ],
            ),
            text(" "),
            node(
                "div",
                "",
                vec![
                    node("span", "", vec![text("c")]),
                    text(" "),
                    node("b", "", vec![text("d")]),
                ],
            ),
        ],
    )]);
    assert_eq!(
        records,
        [
            Record::RowStart(vec![SHARE, SHARE], vec![]),
            para(vec![r(0, "a")]),
            para(vec![r(0, "b")]),
            Record::RowEnd,
            Record::Spacer(10.0),
            para(vec![r(0, "c "), r(1, "d")]),
        ]
    );
}

#[test]
fn given_a_unitless_line_height_on_a_block_when_a_child_changes_font_size_then_each_run_scales_the_factor_by_its_own_size(
) {
    let records = records(vec![node(
        "p",
        "line-height: 1.5",
        vec![text("a"), node("span", "font-size: 20pt", vec![text("b")])],
    )]);
    let a = Run {
        line_height: 18.0,
        ..r(0, "a")
    };
    let b = Run {
        size: 20.0,
        line_height: 30.0,
        ..r(0, "b")
    };
    assert_eq!(records, [Record::Spacer(12.0), para(vec![a, b])]);
}

#[test]
fn given_em_i_and_cite_when_compiling_then_they_default_to_the_helvetica_oblique_slot() {
    let records = records(vec![node(
        "p",
        "",
        vec![
            node("em", "", vec![text("a")]),
            node("i", "", vec![text("b")]),
            node("cite", "", vec![text("c")]),
        ],
    )]);
    assert_eq!(records, [Record::Spacer(12.0), para(vec![r(2, "abc")])]);
}

#[test]
fn given_bold_italic_text_in_a_family_with_only_regular_and_bold_faces_when_compiling_then_the_run_slants_the_bold_face(
) {
    let mut fonts = helvetica();
    fonts.insert(
        "Abel".into(),
        FontSlots {
            regular: 4,
            bold: Some(5),
            italic: None,
            bold_italic: None,
        },
    );
    let nodes = vec![node(
        "p",
        "font-family: Abel; font-style: italic; font-weight: bold",
        vec![text("x")],
    )];
    let records = compiled_with(nodes, &fonts, false).unwrap().records;
    let slanted = Run {
        oblique: true,
        ..r(5, "x")
    };
    assert_eq!(records, [Record::Spacer(12.0), para(vec![slanted])]);
}

#[test]
fn given_capitalize_over_a_word_split_across_runs_when_compiling_then_only_word_starts_are_uppercased(
) {
    let records = records(vec![node(
        "p",
        "text-transform: capitalize",
        vec![
            text("don't stop hel"),
            node("b", "", vec![text("lo")]),
            text(" e-mail"),
        ],
    )]);
    let capitalized = |font, value| Run {
        transform: Transform::Capitalize,
        ..r(font, value)
    };
    assert_eq!(
        records,
        [
            Record::Spacer(12.0),
            para(vec![
                capitalized(0, "Don't Stop Hel"),
                capitalized(1, "lo"),
                capitalized(0, " E-Mail"),
            ])
        ]
    );
}

#[test]
fn given_letter_spacing_in_em_on_a_parent_when_a_child_changes_font_size_then_the_child_inherits_the_parents_points(
) {
    let records = records(vec![node(
        "p",
        "letter-spacing: 0.5em; word-spacing: -1pt",
        vec![node("span", "font-size: 24pt", vec![text("x")])],
    )]);
    let spaced = Run {
        size: 24.0,
        letter_spacing: 6.0,
        word_spacing: -1.0,
        ..r(0, "x")
    };
    assert_eq!(records, [Record::Spacer(12.0), para(vec![spaced])]);
}

#[test]
fn given_a_text_decoration_shorthand_without_a_colour_inside_one_with_a_colour_when_compiling_then_it_resets_the_decoration_colour(
) {
    let records = records(vec![node(
        "p",
        "text-decoration: underline red",
        vec![
            text("a"),
            node("span", "text-decoration: line-through", vec![text("b")]),
        ],
    )]);
    let a = Run {
        underline: true,
        decoration_color: Some([255, 0, 0]),
        ..r(0, "a")
    };
    let b = Run {
        line_through: true,
        ..r(0, "b")
    };
    assert_eq!(records, [Record::Spacer(12.0), para(vec![a, b])]);
}

fn li(children: Vec<Node>) -> Node {
    node("li", "", children)
}

/// Each paragraph's first run: its marker, or its text when the item owes none.
fn firsts(records: &[Record]) -> Vec<(u8, String)> {
    paragraphs(records)
        .into_iter()
        .map(|runs| (runs[0].marker, runs[0].text.clone()))
        .collect()
}

#[test]
fn given_a_ul_and_an_ol_with_start_reversed_and_an_li_value_when_compiling_then_each_items_first_run_is_a_marker_that_numbers_from_them(
) {
    let item = |value: &str| li(vec![text(value)]);
    let records = records(vec![node(
        "main",
        "",
        vec![
            node("ul", "", vec![item("a"), item("b")]),
            with(
                el(
                    "ol",
                    "",
                    vec![
                        item("c"),
                        with(el("li", "", vec![text("d")]), |li| li.value = Some(10)),
                        item("e"),
                    ],
                ),
                |ol| ol.start = Some(3),
            ),
            with(el("ol", "", vec![item("f"), item("g")]), |ol| {
                ol.reversed = true
            }),
        ],
    )]);
    let markers: Vec<_> = paragraphs(&records)
        .into_iter()
        .map(|runs| (runs[0].marker, runs[0].text.clone(), runs[1].text.clone()))
        .collect();
    let expected = [
        ("\u{2022}", "a"),
        ("\u{2022}", "b"),
        ("3. ", "c"),
        ("10. ", "d"),
        ("11. ", "e"),
        ("2. ", "f"),
        ("1. ", "g"),
    ]
    .map(|(marker, value)| (1, marker.to_owned(), value.to_owned()));
    assert_eq!(markers, expected);
}

#[test]
fn given_lists_nested_three_deep_when_compiling_then_ul_markers_cycle_disc_circle_square_and_ol_stays_decimal(
) {
    let list = |tag: &str, children| node(tag, "", children);
    let records = records(vec![list(
        "ul",
        vec![li(vec![
            text("a"),
            list(
                "ul",
                vec![li(vec![text("b"), list("ul", vec![li(vec![text("c")])])])],
            ),
        ])],
    )]);
    let ordered = self::records(vec![list(
        "ol",
        vec![li(vec![text("x"), list("ul", vec![li(vec![text("y")])])])],
    )]);
    let marker = |records: &[Record]| -> Vec<String> {
        firsts(records).into_iter().map(|(_, text)| text).collect()
    };
    assert_eq!(marker(&records), ["\u{2022}", "\u{25e6}", "\u{25aa}"]);
    assert_eq!(marker(&ordered), ["1. ", "\u{25e6}"]);
}

#[test]
fn given_list_style_position_inside_and_type_none_when_compiling_then_the_marker_is_inside_or_absent_while_the_count_goes_on(
) {
    let records = records(vec![node(
        "ol",
        "list-style: upper-roman inside",
        vec![
            li(vec![text("a")]),
            node("li", "list-style-type: none", vec![text("b")]),
            li(vec![text("c")]),
        ],
    )]);
    assert_eq!(
        firsts(&records),
        [(2, "I. ".into()), (0, "b".into()), (2, "III. ".into())]
    );
}

#[test]
fn given_an_li_outside_a_list_a_ul_with_another_child_or_a_value_in_a_ul_when_compiling_then_rejects(
) {
    assert_eq!(
        rejects(vec![li(vec![text("x")])]),
        "<li> belongs in a <ul> or <ol> on <li>"
    );
    assert_eq!(
        rejects(vec![node("ul", "", vec![node("p", "", vec![text("x")])])]),
        "<ul> accepts only <li> on <ul>"
    );
    let valued = with(el("li", "", vec![text("x")]), |li| li.value = Some(2));
    assert_eq!(
        rejects(vec![node("ul", "", vec![valued])]),
        "value applies only to an <li> in an <ol> on <li> in <ul>"
    );
}

#[test]
fn given_an_item_that_starts_with_a_nested_list_when_compiling_then_its_marker_takes_a_line_of_its_own(
) {
    let records = records(vec![node(
        "ul",
        "",
        vec![li(vec![node("ul", "", vec![li(vec![text("in")])])])],
    )]);
    let lines: Vec<_> = paragraphs(&records)
        .into_iter()
        .map(|runs| (runs[0].marker, runs[0].text.clone(), runs[1].text.clone()))
        .collect();
    assert_eq!(
        lines,
        [
            (1, "\u{2022}".into(), "\u{a0}".into()),
            (1, "\u{25e6}".into(), "in".into())
        ]
    );
}

#[test]
fn given_pre_text_with_tabs_runs_of_spaces_and_newlines_when_compiling_then_each_line_is_a_run_that_keeps_its_spaces_and_tabs_reach_multiples_of_8(
) {
    let records = records(vec![node("pre", "", vec![text("\nab\tc  d\n\nend")])]);
    let lines: Vec<_> = paragraphs(&records)[0]
        .iter()
        .map(|run| (run.text.clone(), run.hard_break, run.font))
        .collect();
    let nbsp = "\u{a0}";
    assert_eq!(
        lines,
        [
            (format!("ab{}c{}d", nbsp.repeat(6), nbsp.repeat(2)), true, 8),
            (String::new(), true, 8),
            ("end".into(), false, 8),
        ]
    );
}

#[test]
fn given_pre_wrap_pre_line_and_nowrap_when_compiling_then_spaces_keep_one_wrap_point_collapse_or_never_break(
) {
    let lines = |mode: &str, value: &str| -> Vec<String> {
        let style = format!("white-space: {mode}");
        paragraphs(&records(vec![node("p", &style, vec![text(value)])]))[0]
            .iter()
            .map(|run| run.text.clone())
            .collect()
    };
    assert_eq!(
        lines("pre-wrap", "  a   b\nc"),
        ["\u{a0}\u{a0}a\u{a0}\u{a0} b", "c"]
    );
    assert_eq!(lines("pre-line", "a   b\n c"), ["a   b", " c"]);
    assert_eq!(lines("nowrap", "  a \n  b  "), ["a\u{a0}b"]);
}

#[test]
fn given_sub_and_sup_inside_each_other_and_a_mark_when_compiling_then_the_rise_accumulates_the_size_shrinks_and_the_mark_carries_its_background(
) {
    let records = records(vec![node(
        "p",
        "",
        vec![
            text("x"),
            node("sup", "", vec![text("2"), node("sub", "", vec![text("i")])]),
            node("mark", "", vec![text("m")]),
        ],
    )]);
    let runs = &paragraphs(&records)[0];
    let [x, two, i, mark] = [&runs[0], &runs[1], &runs[2], &runs[3]];
    // A sup lifts a third of the parent's size and a nested sub lowers a fifth of the sup's.
    assert_eq!([x.rise, two.rise], [0.0, 0.33 * 12.0]);
    assert!((i.rise - (0.33 * 12.0 - 0.2 * two.size)).abs() < 1e-9);
    assert!((two.size - 12.0 * 0.83).abs() < 1e-5 && i.size < two.size);
    assert_eq!(
        (mark.background, mark.color),
        (Some([255, 255, 0]), [0, 0, 0])
    );
}

#[test]
fn given_vertical_align_on_a_block_a_box_or_with_an_unknown_value_when_compiling_then_rejects() {
    assert!(
        rejects(vec![node("p", "vertical-align: sub", vec![text("x")])])
            .starts_with("vertical-align applies only to inline elements")
    );
    assert!(rejects(vec![node("sup", "padding: 2pt", vec![text("x")])])
        .starts_with("vertical-align: super shifts text within a line, and <sup> has box styles"));
    let middle = node("span", "vertical-align: middle", vec![text("x")]);
    assert!(rejects(vec![node("p", "", vec![middle])]).starts_with("invalid verticalAlign: middle"));
}

#[test]
fn given_a_p_with_a_10pt_bottom_margin_before_an_h2_with_a_16pt_top_margin_when_compiling_then_one_16pt_spacer_separates_them(
) {
    let records = records(vec![node(
        "main",
        "",
        vec![
            node("p", "margin-bottom: 10pt", vec![text("a")]),
            node("h2", "margin-top: 16pt", vec![text("b")]),
        ],
    )]);
    assert_eq!(
        names(&records),
        ["spacer", "paragraph", "spacer", "paragraph", "avoid"]
    );
    assert_eq!(
        [&records[0], &records[2]],
        [&Record::Spacer(12.0), &Record::Spacer(16.0)]
    );
}

#[test]
fn given_a_box_with_a_left_border_and_bottom_padding_when_compiling_then_a_childs_top_margin_collapses_through_it_and_its_bottom_margin_stays_inside(
) {
    let records = records(vec![node(
        "main",
        "",
        vec![
            node(
                "blockquote",
                "margin: 8pt 0; border-left: 2pt solid; padding-bottom: 4pt",
                vec![node("p", "margin: 12pt 0", vec![text("a")])],
            ),
            text("b"),
        ],
    )]);
    assert_eq!(
        names(&records),
        [
            "spacer",
            "boxStart",
            "paragraph",
            "spacer",
            "boxEnd",
            "spacer",
            "paragraph"
        ]
    );
    let margin = boxes(&records)[0].margin;
    assert_eq!((margin[0], margin[2]), (0.0, 0.0));
    let spacers: Vec<_> = records
        .iter()
        .filter(|record| matches!(record, Record::Spacer(_)))
        .collect();
    assert_eq!(
        spacers,
        [
            &Record::Spacer(12.0),
            &Record::Spacer(12.0),
            &Record::Spacer(8.0)
        ]
    );
}

#[test]
fn given_an_empty_bordered_div_with_20pt_margins_between_paragraphs_when_compiling_then_its_margins_collapse_with_theirs_and_it_writes_no_box(
) {
    let records = records(vec![node(
        "main",
        "",
        vec![
            node("p", "margin-bottom: 8pt", vec![text("a")]),
            node("div", "margin: 20pt 0; border-left: 1pt solid", vec![]),
            node("p", "margin-top: 8pt", vec![text("b")]),
        ],
    )]);
    assert_eq!(
        names(&records),
        ["spacer", "paragraph", "spacer", "paragraph"]
    );
    assert_eq!(records[2], Record::Spacer(20.0));
}

#[test]
fn given_flex_column_items_with_margins_when_compiling_then_their_margins_and_their_childrens_add_up(
) {
    let records = records(vec![node(
        "main",
        "display: flex; flex-direction: column",
        vec![
            node("div", "margin-bottom: 10pt", vec![text("a")]),
            node(
                "div",
                "",
                vec![node("p", "margin-top: 6pt", vec![text("b")])],
            ),
        ],
    )]);
    assert_eq!(
        names(&records),
        [
            "boxStart",
            "paragraph",
            "boxEnd",
            "spacer",
            "paragraph",
            "spacer"
        ]
    );
    let margin = boxes(&records)[0].margin;
    assert_eq!((margin[0], margin[2]), (0.0, 10.0));
    assert_eq!(
        (&records[3], &records[5]),
        (&Record::Spacer(6.0), &Record::Spacer(12.0))
    );
}

#[test]
fn given_margins_around_a_forced_page_break_when_compiling_then_drops_the_one_before_and_keeps_the_one_after(
) {
    let records = records(vec![node(
        "main",
        "",
        vec![
            node("p", "margin-bottom: 20pt", vec![text("a")]),
            node("p", "break-before: page; margin-top: 12pt", vec![text("b")]),
        ],
    )]);
    assert_eq!(
        names(&records),
        ["spacer", "paragraph", "pageBreak", "spacer", "paragraph"]
    );
    assert_eq!(records[3], Record::Spacer(12.0));
}

#[test]
fn given_nested_opacity_when_compiling_then_each_box_run_and_image_carries_the_product_down_the_tree(
) {
    let image = with(el("img", "opacity: 0.5", vec![]), |img| {
        img.picture = Some(Picture::Image(0));
    });
    let records = records(vec![node(
        "div",
        "opacity: 0.5; background-color: #eee; padding: 4pt",
        vec![
            node("p", "", vec![text("a")]),
            node("p", "opacity: 50%", vec![text("b")]),
            image,
        ],
    )]);
    assert_eq!(boxes(&records)[0].opacity, 0.5);
    let opacities: Vec<_> = paragraphs(&records)
        .iter()
        .map(|runs| runs[0].opacity)
        .collect();
    assert_eq!(opacities, [0.5, 0.25]);
    let image = records.iter().find_map(|record| match record {
        Record::Image { opacity, .. } => Some(*opacity),
        _ => None,
    });
    assert_eq!(image, Some(0.25));
}

/// svg.ts's drawing of `<svg width=10 height=10><rect width=10 height=10 fill="red"/></svg>`.
fn red_square() -> Drawing {
    Drawing {
        width: 7.5,
        height: 7.5,
        view_box: [0.0, 0.0, 10.0, 10.0],
        align: Some([0.5, 0.5]),
        slice: false,
        shapes: vec![Shape {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            fill: Some(Paint::Rgb([255, 0, 0])),
            even_odd: false,
            fill_opacity: 1.0,
            stroke: None,
            stroke_width: 1.0,
            cap: 0,
            join: 0,
            miter: 4.0,
            dashes: vec![],
            dash_offset: 0.0,
            stroke_opacity: 1.0,
            ops: vec![0, 1, 1, 1, 3],
            points: vec![0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0],
        }],
    }
}

#[test]
fn given_an_svg_with_opacity_when_compiling_then_the_drawing_holds_the_opacity_and_the_image_record_stays_opaque(
) {
    let svg = with(el("svg", "opacity: 0.5", vec![]), |svg| {
        svg.picture = Some(Picture::Drawing(red_square()));
    });
    let twice = vec![svg.clone(), svg];
    let collected = compiled_with(twice, &helvetica(), false).unwrap();
    // 12 header bytes, then the shape's flags, a 24-byte matrix, and the fill's RGB before its alpha.
    assert_eq!(
        collected.images.len(),
        1,
        "an identical drawing registers once"
    );
    assert_eq!(collected.images[0][12 + 1 + 24 + 3], 128);
    let images: Vec<_> = collected
        .records
        .iter()
        .filter_map(|record| match record {
            Record::Image { slot, opacity, .. } => Some((*slot, *opacity)),
            _ => None,
        })
        .collect();
    assert_eq!(images, [(100, 1.0), (100, 1.0)]);
}

#[test]
fn given_a_border_radius_with_elliptical_and_percent_corners_when_compiling_then_emits_eight_radii_and_a_percent_mask(
) {
    let records = records(vec![node(
        "div",
        "background-color: #eee; border-radius: 4pt 8pt / 50% 2pt",
        vec![text("x")],
    )]);
    let radius = boxes(&records)[0].radius.unwrap();
    assert_eq!(radius.values, [4.0, 50.0, 8.0, 2.0, 4.0, 50.0, 8.0, 2.0]);
    assert_eq!(radius.percent, 0b100010);
}

#[test]
fn given_a_radius_on_an_img_when_compiling_then_rejects_because_the_picture_would_not_be_clipped() {
    let image = with(el("img", "border-radius: 4pt", vec![]), |img| {
        img.picture = Some(Picture::Image(0));
    });
    let message = rejects(vec![image]);
    assert!(
        message.starts_with("border-radius does not clip <img>") && message.ends_with(" on <img>")
    );
}

#[test]
fn given_a_link_target_and_headings_with_an_outline_when_compiling_the_body_then_an_anchor_precedes_each_target_and_each_bookmarked_heading(
) {
    let link = with(el("a", "", vec![text("Terms")]), |a| {
        a.href = Some("#terms".into())
    });
    let heading = |tag: &str, bookmark, value| {
        with(el(tag, "", vec![text(value)]), |h| h.bookmark = bookmark)
    };
    let section = with(
        el(
            "section",
            "",
            vec![
                node(
                    "h2",
                    "",
                    vec![
                        text("Terms "),
                        node("b", "", vec![text("and")]),
                        node("br", "", vec![]),
                        text(" conditions"),
                    ],
                ),
                heading("h3", Bookmark::Off, "Skipped"),
                heading("h3", Bookmark::Title("Late fees".into()), "Fees"),
                with(el("h4", "", vec![text("   ")]), |h| {
                    h.id = Some("unlinked".into())
                }),
            ],
        ),
        |section| section.id = Some("terms".into()),
    );
    let nodes = vec![node("p", "", vec![link]), section];
    let records = compiled_with(nodes, &helvetica(), true).unwrap().records;
    let shown: Vec<_> = records
        .iter()
        .filter_map(|record| match record {
            Record::Anchor { name, level, title } => Some(format!("{name}|{level}|{title}")),
            Record::Paragraph { .. } => Some("paragraph".into()),
            _ => None,
        })
        .collect();
    assert_eq!(
        shown,
        [
            "paragraph",
            "terms|0|",
            "|2|Terms and conditions",
            "paragraph",
            "paragraph",
            "|3|Late fees",
            "paragraph",
            "paragraph",
        ]
    );
    let plain = compiled_with(
        vec![node("h1", "", vec![text("Title")])],
        &helvetica(),
        false,
    );
    assert!(
        !names(&plain.unwrap().records).contains(&"anchor"),
        "without the outline option or a link target, no anchor is written"
    );
}

#[test]
fn given_a_link_to_an_id_that_is_missing_repeated_or_on_an_inline_span_when_compiling_the_body_then_rejects_naming_the_id_and_element(
) {
    let link = |id: &str| {
        with(el("a", "", vec![text("x")]), |a| {
            a.href = Some(format!("#{id}"))
        })
    };
    let owner = |value| with(el("p", "", vec![text(value)]), |p| p.id = Some("a".into()));
    assert_eq!(
        rejects(vec![link("nope")]),
        "link to #nope: no element has id=\"nope\" on <a>"
    );
    assert_eq!(
        rejects(vec![owner("1"), owner("2"), link("a")]),
        "link to #a: id=\"a\" is on more than one element on <p#a>"
    );
    assert!(compiled_with(vec![owner("1"), owner("2")], &helvetica(), false).is_ok());
    let span = with(el("span", "", vec![text("this")]), |span| {
        span.id = Some("n".into())
    });
    let message = rejects(vec![node("p", "", vec![text("see "), span]), link("n")]);
    assert!(message.starts_with("link to #n: its element cannot hold a destination"));
    assert!(message.ends_with(" on <span#n> in <p>"));
}

#[test]
fn given_more_than_10000_bookmarked_headings_when_compiling_the_body_then_rejects_before_the_engine_does(
) {
    let headings = (0..10_001)
        .map(|_| node("h2", "", vec![text("x")]))
        .collect();
    let message = compiled_with(headings, &helvetica(), true)
        .err()
        .unwrap()
        .into_message();
    assert_eq!(
        message,
        "more than 10000 link targets and bookmarks on <h2>"
    );
}

#[test]
fn given_justify_content_on_a_row_when_compiling_then_emits_one_flex_record_and_keeps_each_items_width_on_its_own_box(
) {
    let records = records(vec![node(
        "div",
        "display: flex; justify-content: space-between; align-items: center",
        vec![
            node("div", "width: 80pt", vec![text("Logo")]),
            node("p", "flex: 1 0 50%", vec![text("Due")]),
        ],
    )]);
    assert_eq!(
        records[0],
        Record::FlexStart(
            Flex {
                column: false,
                justify: 3,
                align: 4,
                gap: 0.0,
                min_height: 0.0,
                max_height: None
            },
            vec![
                FlexItem {
                    grow: 0.0,
                    shrink: 1.0,
                    basis: None,
                    align: 0
                },
                FlexItem {
                    grow: 1.0,
                    shrink: 0.0,
                    basis: Some(Dimension {
                        kind: 2,
                        value: 50.0
                    }),
                    align: 0
                },
            ]
        )
    );
    let width = boxes(&records)[0].size.and_then(|size| size.width);
    assert_eq!(
        width,
        Some(Dimension {
            kind: 1,
            value: 80.0
        })
    );
    assert_eq!(
        names(&records[1..]),
        [
            "boxStart",
            "paragraph",
            "boxEnd",
            "boxStart",
            "paragraph",
            "boxEnd",
            "flexEnd"
        ]
    );
}

#[test]
fn given_a_flex_column_without_a_height_when_compiling_justify_content_or_a_grow_factor_then_rejects_with_the_reason(
) {
    let column = "display: flex; flex-direction: column";
    assert_eq!(
        rejects(vec![node(
            "div",
            &format!("{column}; justify-content: center"),
            vec![text("x")]
        )]),
        "justify-content in a flex column needs a height or min-height on the column on <div>"
    );
    let grown = node("p", "flex: 1", vec![text("x")]);
    assert_eq!(
        rejects(vec![node(
            "div",
            &format!("{column}; height: 100pt"),
            vec![grown]
        )]),
        "flex-grow and flex-basis size flex row items; give a column item a height on <p> in <div>"
    );
}

/// Fails the first paragraph with an engine code, as the decoder does.
struct Failing(&'static str);

impl Sink for Failing {
    fn record(&mut self, record: &Record, _: &[u8]) -> std::result::Result<(), String> {
        match record {
            Record::Paragraph { .. } => Err(self.0.into()),
            _ => Ok(()),
        }
    }
    fn add_image(&mut self, _: Vec<u8>) -> std::result::Result<u16, String> {
        Ok(0)
    }
    fn add_raster(&mut self, handle: u16) -> std::result::Result<u16, String> {
        Ok(handle)
    }
}

fn engine_failure(code: &'static str, nodes: Vec<Node>) -> String {
    let fonts = helvetica();
    let mut compiler = Compiler::new(Styler::new(&[], 523.0).unwrap(), fonts, Failing(code));
    compiler.body(&nodes, false).err().unwrap().into_message()
}

#[test]
fn given_an_engine_error_while_writing_when_compiling_then_explains_it_and_names_the_element() {
    let nodes = || vec![node("div", "", vec![node("p", "", vec![text("x")])])];
    assert_eq!(
        engine_failure("MissingGlyph('\\u{301}')", nodes()),
        "the font has no glyph for \"\u{301}\" (U+0301) on <p> in <div>"
    );
    assert!(engine_failure("TextTooWide", nodes()).starts_with("TextTooWide: a character is wider"));
    assert_eq!(
        engine_failure("PageOverflow", nodes()),
        "PageOverflow: the content does not fit on an empty page; split it into smaller blocks on <p> in <div>"
    );
    let decorated = vec![node("div", "border: 1pt solid", vec![text("x")])];
    assert_eq!(
        engine_failure("PageOverflow", decorated),
        "PageOverflow: use document margins or split this decorated box on <div>"
    );
}

#[test]
fn given_a_paragraph_over_64_kib_when_compiling_then_rejects_with_a_split_hint() {
    let long = vec![node("p", "", vec![text(&"a".repeat(70_000))])];
    assert_eq!(
        rejects(long),
        "paragraph exceeds 64 KiB; split it into several paragraphs on <p>"
    );
}

#[test]
fn given_page_breaks_past_10000_pages_when_compiling_then_rejects_before_the_engine_sees_the_extra_page(
) {
    let pages = (0..10_001)
        .map(|_| node("p", "break-before: page", vec![text("x")]))
        .collect();
    assert!(
        rejects(pages).starts_with("document exceeds 10000 pages; split it across several renders")
    );
}
