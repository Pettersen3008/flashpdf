use crate::{
    BoxSize, BoxStyle, ColumnWidth, Flex, FlexItem, ImageSize, Pt, TextAlign, TextRun,
    VerticalAlign,
};

pub(crate) struct Document<'a> {
    pub(crate) blocks: Vec<Block<'a>>,
}

pub(crate) enum Block<'a> {
    Element(Element<'a>),
    PageBreak,
    Avoid,
}

#[derive(Clone)]
pub(crate) enum Element<'a> {
    Position(PositionNode<'a>),
    Paragraph(Paragraph<'a>),
    Box(BoxNode<'a>),
    Spacer(Spacer),
    Stack(Stack<'a>),
    Row(Row<'a>),
    Table(Table<'a>),
    Image(ImageNode<'a>),
    Flex(FlexNode<'a>),
}

#[derive(Clone)]
pub(crate) struct Paragraph<'a> {
    pub(crate) align: TextAlign,
    /// Lines a page break leaves before it and after it, at least 1.
    pub(crate) orphans: u16,
    pub(crate) widows: u16,
    pub(crate) text: &'a str,
    pub(crate) runs: Vec<TextRun>,
    pub(crate) links: &'a [String],
}

#[derive(Clone)]
pub(crate) struct ImageNode<'a> {
    pub(crate) slot: u16,
    pub(crate) width: ImageSize,
    pub(crate) height: Option<Pt>,
    pub(crate) link: Option<&'a str>,
    pub(crate) alt: &'a str,
}

#[derive(Clone)]
pub(crate) struct Spacer {
    pub(crate) height: Pt,
}

#[derive(Clone)]
pub(crate) struct BoxNode<'a> {
    pub(crate) style: BoxStyle,
    pub(crate) size: BoxSize,
    pub(crate) children: Vec<Element<'a>>,
}

#[derive(Clone)]
pub(crate) struct Stack<'a> {
    pub(crate) gap: Pt,
    pub(crate) children: Vec<Element<'a>>,
}

#[derive(Clone)]
pub(crate) struct Row<'a> {
    pub(crate) columns: &'a [ColumnWidth],
    pub(crate) cells: Vec<Cell<'a>>,
}

/// `items` and `children` pair up by index.
#[derive(Clone)]
pub(crate) struct FlexNode<'a> {
    pub(crate) style: Flex,
    pub(crate) items: &'a [FlexItem],
    pub(crate) children: Vec<Element<'a>>,
}

/// Covers `span` columns from `column` and `rows` rows down.
#[derive(Clone)]
pub(crate) struct Cell<'a> {
    pub(crate) column: usize,
    pub(crate) span: usize,
    pub(crate) rows: u16,
    pub(crate) align: VerticalAlign,
    pub(crate) content: Element<'a>,
}

/// Rows are Row elements, each optionally wrapped in a Box (`tr` paint) or a
/// Stack (`break-inside: avoid` group, or rows a rowspan joins). The first `header_rows`, then the next
/// `footer_rows`, repeat per page; body rows follow them.
#[derive(Clone)]
pub(crate) struct Table<'a> {
    pub(crate) header_rows: usize,
    pub(crate) footer_rows: usize,
    pub(crate) width: ColumnWidth,
    pub(crate) rows: Vec<Element<'a>>,
}

#[derive(Clone)]
pub(crate) struct PositionNode<'a> {
    pub(crate) style: crate::Position,
    pub(crate) content: Box<Element<'a>>,
}
