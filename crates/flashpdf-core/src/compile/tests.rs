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
    let mut compiler = Compiler::new(styler, fonts, Collect::default());
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

fn texts(records: &[Record]) -> Vec<String> {
    paragraphs(records)
        .iter()
        .map(|runs| runs.iter().map(|run| run.text.as_str()).collect())
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
