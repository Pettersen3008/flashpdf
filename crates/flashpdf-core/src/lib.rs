mod font;
mod layout;
mod model;
mod pdf;

pub mod protocol;

pub(crate) use font::EmbeddedFont;
pub(crate) use layout::OwnedCommand;
pub use layout::{render, Renderer};
pub use model::*;
#[cfg(test)]
mod tests {
    use super::{Command, Page, Pt, RenderError};

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
                    text: "WW",
                    size: Pt::new(30.0).unwrap(),
                }]
            ),
            Err(RenderError::TextTooWide)
        );
        assert_eq!(
            super::render(
                Page::A4,
                &[Command::Text {
                    text: "\u{1f642}",
                    size: Pt::new(12.0).unwrap(),
                }]
            ),
            Err(RenderError::UnsupportedCharacter('\u{1f642}'))
        );

        let commands = [
            Command::Text {
                text: "First \u{20ac}",
                size: Pt::new(14.0).unwrap(),
            },
            Command::Spacer(Pt::new(12.0).unwrap()),
            Command::Text {
                text: "After spacer",
                size: Pt::new(10.0).unwrap(),
            },
            Command::PageBreak,
            Command::Text {
                text: "Second page",
                size: Pt::new(12.0).unwrap(),
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
        assert!(String::from_utf8(pdf.get_page_content((4, 0)).unwrap())
            .unwrap()
            .contains("36 780 Td"));
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

    fn page() -> Page {
        Page::new(Pt(120.0), Pt(60.0), Pt(10.0)).unwrap()
    }

    fn text(text: &str) -> Command<'_> {
        Command::Text {
            text,
            size: Pt(10.0),
        }
    }

    fn parsed(commands: &[Command<'_>]) -> lopdf::Document {
        lopdf::Document::load_mem(&super::render(page(), commands).unwrap()).unwrap()
    }

    #[test]
    fn given_exact_bottom_text_when_placed_then_stays_on_page() {
        let pdf = parsed(&[Command::Spacer(Pt(30.0)), text("bottom")]);
        assert_eq!(pdf.get_pages().len(), 1);
        assert!(String::from_utf8(pdf.get_page_content((4, 0)).unwrap())
            .unwrap()
            .contains("10 20 Td"));
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
        assert!(pdf.get_page_content((4, 0)).unwrap().is_empty());
    }

    #[test]
    fn given_oversized_block_when_measured_then_rejects() {
        assert_eq!(
            super::render(
                page(),
                &[
                    Command::StackStart { gap: Pt(21.0) },
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
        let columns = [Fixed(Pt(20.0)), Fraction(Pt(1.0)), Fraction(Pt(3.0))];
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
        let content = String::from_utf8(pdf.get_page_content((4, 0)).unwrap()).unwrap();
        for position in [
            "10 50 Td", "10 40 Td", "30 50 Td", "30 40 Td", "50 50 Td", "10 30 Td",
        ] {
            assert!(content.contains(position), "{position}: {content}");
        }
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
                    columns: &[Fraction(Pt(0.0))],
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
        ] {
            assert_eq!(
                super::render(page(), &commands),
                Err(RenderError::InvalidLayout)
            );
        }
        let too_many = [Fraction(Pt(1.0)); 257];
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
}
