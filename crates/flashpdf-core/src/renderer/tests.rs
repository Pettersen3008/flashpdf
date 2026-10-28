use crate::{
    BoxSize, BoxStyle, ColumnWidth, Command, Edges, OwnedCommand, Page, Pt, Renderer, TextAlign,
    TextRun, TextStyle,
};

fn first_header(renderer: &mut Renderer) {
    let run = |hard_break| TextRun {
        len: 1,
        font: crate::HELVETICA,
        size: Pt(8.0),
        line_height: None,
        color: crate::Rgb::BLACK,
        hard_break,
        decoration: Default::default(),
        typography: crate::Typography::NORMAL,
    };
    renderer
        .set_furniture(
            false,
            true,
            vec![OwnedCommand::Paragraph(
                (TextAlign::Left, crate::Direction::Ltr),
                [2, 2],
                "ab".into(),
                vec![run(true), run(false)],
                vec![],
            )],
        )
        .unwrap();
}

fn renderer() -> Renderer {
    let mut renderer = Renderer::new(Page::new(Pt(100.0), Pt(60.0), Edges::all(Pt(10.0))).unwrap());
    first_header(&mut renderer);
    renderer
}

#[test]
fn given_a_tall_first_header_when_an_atomic_block_only_fits_a_later_page_then_moves_it_whole() {
    let mut renderer = renderer();
    renderer
        .push(&[
            Command::StackStart { gap: Pt::ZERO },
            Command::Spacer(Pt(20.0)),
            Command::Text {
                text: "body",
                style: TextStyle::plain(Pt(10.0)),
            },
            Command::StackEnd,
        ])
        .unwrap();
    let pdf = lopdf::Document::load_mem(&renderer.finish().unwrap()).unwrap();
    assert_eq!(pdf.get_pages().len(), 2);
    assert!(!pdf.extract_text(&[1]).unwrap().contains("body"));
    assert!(pdf.extract_text(&[2]).unwrap().contains("body"));
}

#[test]
fn given_a_tall_first_header_when_a_table_row_only_fits_a_later_page_then_moves_it_whole() {
    let mut renderer = renderer();
    renderer
        .push(&[
            Command::TableStart {
                header_rows: 0,
                footer_rows: 0,
                width: ColumnWidth::Fraction(crate::Fraction::new(1.0).unwrap()),
            },
            Command::RowStart {
                columns: &[ColumnWidth::Fixed(Pt(80.0))],
                slots: &[],
            },
            Command::StackStart { gap: Pt::ZERO },
            Command::Spacer(Pt(20.0)),
            Command::Text {
                text: "body",
                style: TextStyle::plain(Pt(10.0)),
            },
            Command::StackEnd,
            Command::RowEnd,
            Command::TableEnd,
        ])
        .unwrap();
    let pdf = lopdf::Document::load_mem(&renderer.finish().unwrap()).unwrap();
    assert_eq!(pdf.get_pages().len(), 2);
    assert!(pdf.extract_text(&[2]).unwrap().contains("body"));
}

#[test]
fn given_a_split_box_and_different_first_header_when_continuing_then_background_starts_at_the_later_body_top(
) {
    let mut renderer = renderer();
    renderer
        .push(&[
            Command::BoxStart {
                style: background(),
                size: BoxSize::AUTO,
            },
            Command::Text {
                text: "one",
                style: TextStyle::plain(Pt(10.0)),
            },
            Command::Text {
                text: "two",
                style: TextStyle::plain(Pt(10.0)),
            },
            Command::Text {
                text: "three",
                style: TextStyle::plain(Pt(10.0)),
            },
            Command::BoxEnd,
        ])
        .unwrap();
    let pdf = lopdf::Document::load_mem(&renderer.finish().unwrap()).unwrap();
    let page = pdf.get_pages()[&2];
    let content = lopdf::content::Content::decode(&pdf.get_page_content(page)).unwrap();
    let rectangle = content
        .operations
        .iter()
        .find(|operation| operation.operator == "re")
        .unwrap();
    let y = rectangle.operands[1].as_float().unwrap();
    let height = rectangle.operands[3].as_float().unwrap();
    assert!((y + height - 50.0).abs() < 0.001);
}

fn background() -> BoxStyle {
    BoxStyle {
        effects: None,
        margin: Edges::all(Pt::ZERO),
        padding: Edges::all(Pt::ZERO),
        border: Edges::all(Pt::ZERO),
        background: Some(crate::Rgb::BLACK),
        border_color: Edges::all(crate::Rgb::BLACK),
        border_style: Edges::all(crate::LineStyle::Solid),
        radius: None,
        alpha: crate::OPAQUE,
    }
}

#[test]
fn given_a_rounded_box_crossing_a_page_when_rendered_then_moves_whole() {
    let mut renderer = Renderer::new(Page::new(Pt(100.0), Pt(60.0), Edges::all(Pt(10.0))).unwrap());
    renderer
        .push(&[
            Command::Spacer(Pt(30.0)),
            Command::BoxStart {
                style: BoxStyle {
                    radius: Some(crate::Radii {
                        values: [2.0; 8],
                        percent: 0,
                    }),
                    ..background()
                },
                size: BoxSize::AUTO,
            },
            Command::Text {
                text: "one",
                style: TextStyle::plain(Pt(10.0)),
            },
            Command::Text {
                text: "two",
                style: TextStyle::plain(Pt(10.0)),
            },
            Command::BoxEnd,
        ])
        .unwrap();
    let pdf = lopdf::Document::load_mem(&renderer.finish().unwrap()).unwrap();
    assert_eq!(pdf.get_pages().len(), 2);
    assert!(pdf.extract_text(&[1]).unwrap().trim().is_empty());
    assert_eq!(pdf.extract_text(&[2]).unwrap().trim(), "one\ntwo");
}

#[test]
fn given_a_smaller_later_body_when_an_atomic_block_fits_neither_remaining_nor_later_space_then_rejects(
) {
    let mut renderer = Renderer::new(Page::new(Pt(100.0), Pt(60.0), Edges::all(Pt(10.0))).unwrap());
    renderer
        .set_furniture(
            false,
            false,
            vec![OwnedCommand::Paragraph(
                (TextAlign::Left, crate::Direction::Ltr),
                [2, 2],
                "later".into(),
                vec![TextRun {
                    len: 5,
                    font: crate::HELVETICA,
                    size: Pt(10.0),
                    line_height: None,
                    color: crate::Rgb::BLACK,
                    hard_break: false,
                    decoration: Default::default(),
                    typography: crate::Typography::NORMAL,
                }],
                vec![],
            )],
        )
        .unwrap();
    renderer
        .set_furniture(
            false,
            true,
            vec![OwnedCommand::StackStart(Pt::ZERO), OwnedCommand::StackEnd],
        )
        .unwrap();
    assert_eq!(
        renderer.push(&[
            Command::Spacer(Pt(10.0)),
            Command::StackStart { gap: Pt::ZERO },
            Command::Spacer(Pt(24.0)),
            Command::Text {
                text: "body",
                style: TextStyle::plain(Pt(10.0))
            },
            Command::StackEnd,
        ]),
        Err(crate::RenderError::PageOverflow)
    );
}

#[test]
fn given_tagged_pdfa_output_when_loading_then_struct_elements_resolve_from_the_object_stream() {
    let mut renderer = Renderer::new(Page::new(Pt(100.0), Pt(60.0), Edges::all(Pt(10.0))).unwrap());
    renderer.set_metadata(crate::pdf::Metadata {
        language: "en".into(),
        tagged: true,
        pdfa: Some(crate::pdf::PdfA::A3b),
        ..Default::default()
    });
    renderer
        .push(&[Command::Text {
            text: "body",
            style: TextStyle::plain(Pt(10.0)),
        }])
        .unwrap();
    let bytes = renderer.finish().unwrap();
    let pdf = lopdf::Document::load_mem(&bytes).unwrap();
    assert!(pdf.trailer.get(b"ID").is_ok());
    let root = pdf.catalog().unwrap().get(b"StructTreeRoot").unwrap();
    let root = pdf.get_dictionary(root.as_reference().unwrap()).unwrap();
    let document = root.get(b"K").unwrap().as_reference().unwrap();
    let kids = pdf.get_dictionary(document).unwrap().get(b"K").unwrap();
    let paragraph = kids.as_array().unwrap()[0].as_reference().unwrap();
    let paragraph = pdf.get_dictionary(paragraph).unwrap();
    assert_eq!(paragraph.get(b"S").unwrap().as_name().unwrap(), b"P");
    assert_eq!(paragraph.get(b"K").unwrap().as_i64().unwrap(), 0);
    assert!(!bytes.windows(10).any(|window| window == b"StructElem"));
}
