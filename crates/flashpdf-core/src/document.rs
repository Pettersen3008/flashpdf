use crate::{BoxStyle, ColumnWidth, Pt, TextAlign, TextRun};

pub(crate) struct Document<'a> {
    pub(crate) blocks: Vec<Block<'a>>,
}

pub(crate) enum Block<'a> {
    Element(Element<'a>),
    PageBreak,
}

pub(crate) enum Element<'a> {
    Paragraph(Paragraph<'a>),
    Box(BoxNode<'a>),
    Spacer(Spacer),
    Stack(Stack<'a>),
    Row(Row<'a>),
}

pub(crate) struct Paragraph<'a> {
    pub(crate) align: TextAlign,
    pub(crate) text: &'a str,
    pub(crate) runs: Vec<TextRun>,
}

pub(crate) struct Spacer {
    pub(crate) height: Pt,
}

pub(crate) struct BoxNode<'a> {
    pub(crate) style: BoxStyle,
    pub(crate) children: Vec<Element<'a>>,
}

pub(crate) struct Stack<'a> {
    pub(crate) gap: Pt,
    pub(crate) children: Vec<Element<'a>>,
}

pub(crate) struct Row<'a> {
    pub(crate) cells: Vec<Cell<'a>>,
}

pub(crate) struct Cell<'a> {
    pub(crate) width: ColumnWidth,
    pub(crate) content: Element<'a>,
}
