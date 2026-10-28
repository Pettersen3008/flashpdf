use std::rc::Rc;

use super::{
    element_column, get, inherit, is, p, row_only, sizeless, static_only, Box, CellPaint, Column,
    Compiler, Ctx, Error, Result, Sink, Slot, Styled, StyledNode, BLACK,
};
use crate::css::style::{border_spacing, box_style, Rgb};

struct RowGroup<'s, 'a> {
    group: Option<&'s Styled<'a>>,
    rows: Vec<&'s Styled<'a>>,
}

/// A `td` or `th` and the grid area it covers; `row` counts from the table's first row.
struct GridCell<'s, 'a> {
    node: &'s Styled<'a>,
    row: usize,
    column: usize,
    columns: usize,
    rows: usize,
}

struct GridRow<'s, 'a> {
    node: &'s Styled<'a>,
    state: Ctx,
    paint: Option<Box>,
    cells: Vec<GridCell<'s, 'a>>,
    slots: Vec<Slot>,
    /// A rowSpan from an earlier row reaches this one.
    joined: bool,
}

struct GridGroup<'s, 'a> {
    group: Option<&'s Styled<'a>>,
    /// Indices into the table's rows.
    rows: Vec<usize>,
}

#[derive(Clone, Copy)]
struct Edge {
    width: f64,
    color: Rgb,
}

const ALIGN: [&str; 3] = ["top", "middle", "bottom"];

/// `thead` groups first, then `tbody` and bare `tr` in source order, then `tfoot`, as browsers
/// paint them. Adjacent bare rows form one group, as HTML's implied `tbody` does.
#[allow(clippy::type_complexity)]
fn row_groups<'s, 'a>(node: &'s Styled<'a>) -> Result<[Vec<RowGroup<'s, 'a>>; 3]> {
    let mut groups: [Vec<RowGroup>; 3] = Default::default();
    let accepts = "<table> accepts only <thead>, <tbody>, <tfoot>, and <tr>";
    for child in &node.children {
        let StyledNode::Element(child) = child else {
            return Err(Error::new(format!("{accepts}, not text")));
        };
        let tag = child.el.tag.as_str();
        if tag == "tr" {
            match groups[1].last_mut() {
                Some(last) if last.group.is_none() => last.rows.push(child),
                _ => groups[1].push(RowGroup {
                    group: None,
                    rows: vec![child],
                }),
            }
            continue;
        }
        let index = match tag {
            "thead" => 0,
            "tbody" => 1,
            "tfoot" => 2,
            _ => return Err(Error::new(format!("{accepts}, not <{tag}>"))),
        };
        let mut rows = Vec::with_capacity(child.children.len());
        for row in &child.children {
            match row {
                StyledNode::Element(row) if row.el.tag == "tr" => rows.push(row),
                _ => return Err(Error::new(format!("<{tag}> accepts only <tr>")).locate(&child.at)),
            }
        }
        groups[index].push(RowGroup {
            group: Some(child),
            rows,
        });
    }
    Ok(groups)
}

fn edge(value: Option<&Box>, side: usize) -> Edge {
    value.map_or(
        Edge {
            width: 0.0,
            color: BLACK,
        },
        |value| Edge {
            width: value.border[side],
            color: value.border_color[side],
        },
    )
}

/// The wider border wins; on a tie the earlier candidate does.
fn widest(edges: impl IntoIterator<Item = Edge>) -> Edge {
    edges
        .into_iter()
        .reduce(|best, next| if next.width > best.width { next } else { best })
        .unwrap()
}

/// CSS 2.1 collapsed borders: each edge takes the widest border of the cells and rows that
/// meet there, a cell's beating its row's on a tie and the upper or left one beating the other.
/// A cell draws half of each shared edge and all of an outer one, so each line draws once.
fn collapse(rows: &[GridRow], width: usize, boxes: &[Vec<Option<Box>>]) -> Vec<Vec<CellPaint>> {
    let mut at: Vec<Vec<Option<(usize, usize)>>> = vec![vec![None; width]; rows.len()];
    for (r, row) in rows.iter().enumerate() {
        for (i, cell) in row.cells.iter().enumerate() {
            for line in &mut at[cell.row..cell.row + cell.rows] {
                for slot in &mut line[cell.column..cell.column + cell.columns] {
                    *slot = Some((r, i));
                }
            }
        }
    }
    let near = |r: usize, c: usize| at[r][c].and_then(|(row, cell)| boxes[row][cell].as_ref());
    let tr = |r: usize| rows[r].paint.as_ref();
    rows.iter()
        .enumerate()
        .map(|(r, row)| {
            row.cells
                .iter()
                .enumerate()
                .map(|(i, cell)| {
                    let own = boxes[r][i].as_ref();
                    let (top, left) = (cell.row, cell.column);
                    let bottom = top + cell.rows;
                    let right = left + cell.columns;
                    let shared = [top > 0, right < width, bottom < rows.len(), left > 0];
                    let across = left..right;
                    let down = top..bottom;
                    // Each side lists every segment's candidates in tie order.
                    let winners = [
                        widest(across.clone().map(|c| {
                            if shared[0] {
                                widest([
                                    edge(near(top - 1, c), 2),
                                    edge(own, 0),
                                    edge(tr(top - 1), 2),
                                    edge(tr(top), 0),
                                ])
                            } else {
                                widest([edge(own, 0), edge(tr(top), 0)])
                            }
                        })),
                        widest(down.clone().map(|r| {
                            if shared[1] {
                                widest([edge(own, 1), edge(near(r, right), 3)])
                            } else {
                                widest([edge(own, 1), edge(tr(r), 1)])
                            }
                        })),
                        widest(across.map(|c| {
                            if shared[2] {
                                widest([
                                    edge(own, 2),
                                    edge(near(bottom, c), 0),
                                    edge(tr(bottom - 1), 2),
                                    edge(tr(bottom), 0),
                                ])
                            } else {
                                widest([edge(own, 2), edge(tr(bottom - 1), 2)])
                            }
                        })),
                        widest(down.map(|r| {
                            if shared[3] {
                                widest([edge(near(r, left - 1), 1), edge(own, 3)])
                            } else {
                                widest([edge(own, 3), edge(tr(r), 3)])
                            }
                        })),
                    ];
                    CellPaint {
                        border: Some(std::array::from_fn(|side| {
                            if shared[side] {
                                winners[side].width / 2.0
                            } else {
                                winners[side].width
                            }
                        })),
                        border_color: Some(winners.map(|winner| winner.color)),
                        background: None,
                    }
                })
                .collect()
        })
        .collect()
}

const PAGE_BREAKS: &str = "page breaks inside tables are not supported; split the table";

impl<S: Sink> Compiler<S> {
    pub(super) fn table(&mut self, node: &Styled, context: &Ctx, depth: usize) -> Result<()> {
        let s = &*node.style;
        if [p!("flexGrow"), p!("flexShrink"), p!("flexBasis")]
            .iter()
            .any(|prop| s.get(*prop).is_some())
        {
            return Err(Error::new("flex applies only to a flex row child"));
        }
        sizeless(s, "table", &["width"])?;
        if context.item && s.get(p!("width")).is_some() {
            return Err(Error::new(
                "a table in a flex container takes no width; set it on a wrapping <div>",
            ));
        }
        self.page_break(s, false, depth)?;
        let next = inherit(s, context, &self.fonts, &mut self.writer.sink, node.el.dir)?;
        let paint = box_style(s, next.size, next.opacity)?;
        if paint.as_ref().is_some_and(|paint| {
            paint.margin[1] != 0.0
                || paint.margin[3] != 0.0
                || paint.background.is_some()
                || paint
                    .padding
                    .iter()
                    .chain(&paint.border)
                    .any(|edge| *edge != 0.0)
        }) {
            return Err(Error::new(
                "<table> accepts only vertical margins; style its cells or a wrapping <div>",
            ));
        }
        let collapsed = is(s, p!("borderCollapse"), "collapse");
        if !collapsed {
            if let Some(value) = get(s, p!("borderSpacing")) {
                if border_spacing(value)? {
                    return Err(Error::new(
                        "border-spacing is not implemented: cells tile their row; use cell padding, or border-collapse: collapse for single rules",
                    ));
                }
            }
        }
        let inner = next;
        let [header, body, footer] = row_groups(node)?;

        // Places each cell in the next column that no rowSpan from an earlier row covers, as HTML does.
        let mut width = 0;
        let mut rows: Vec<GridRow> = Vec::new();
        let mut placed: [Vec<GridGroup>; 3] = Default::default();
        for (index, groups) in [header, body, footer].into_iter().enumerate() {
            for group in groups {
                let state = match group.group {
                    None => inner.clone(),
                    Some(element) => (|| {
                        row_only(&element.style, false)?;
                        static_only(&element.style, &element.el.tag)?;
                        if is(&element.style, p!("breakBefore"), "page")
                            || is(&element.style, p!("breakAfter"), "page")
                        {
                            return Err(Error::new(PAGE_BREAKS));
                        }
                        if box_style(&element.style, inner.size, 1.0)?.is_some() {
                            return Err(Error::new(format!(
                                "<{}> accepts no box styles; style <tr> or its cells",
                                element.el.tag
                            )));
                        }
                        inherit(
                            &element.style,
                            &inner,
                            &self.fonts,
                            &mut self.writer.sink,
                            element.el.dir,
                        )
                    })()
                    .map_err(|error| error.locate(&element.at))?,
                };
                let mut covered: Vec<i64> = Vec::new();
                let mut reach = 0;
                let mut indices = Vec::with_capacity(group.rows.len());
                let count = group.rows.len();
                for (index, tr) in group.rows.iter().enumerate() {
                    let row = (|| {
                        row_only(&tr.style, false)?;
                        static_only(&tr.style, "tr")?;
                        if is(&tr.style, p!("breakBefore"), "page")
                            || is(&tr.style, p!("breakAfter"), "page")
                        {
                            return Err(Error::new(PAGE_BREAKS));
                        }
                        let own = inherit(&tr.style, &state, &self.fonts, &mut self.writer.sink, tr.el.dir)?;
                        let paint = box_style(&tr.style, own.size, own.opacity)?;
                        let mut row = GridRow {
                            node: tr,
                            state: own,
                            paint,
                            cells: Vec::new(),
                            slots: Vec::new(),
                            joined: index < reach,
                        };
                        let skip = |row: &mut GridRow, covered: &[i64]| {
                            while covered.get(row.slots.len()).is_some_and(|count| *count != 0) {
                                row.slots.push(Slot::Covered);
                            }
                        };
                        for cell in &tr.children {
                            let cell = match cell {
                                StyledNode::Element(cell) if matches!(cell.el.tag.as_str(), "td" | "th") => cell,
                                _ => return Err(Error::new("<tr> accepts only <td> and <th>")),
                            };
                            skip(&mut row, &covered);
                            let column = row.slots.len();
                            let columns = usize::from(cell.el.col_span.unwrap_or(1));
                            let span = cell.el.row_span.unwrap_or(1);
                            if index + usize::from(span) > count {
                                let within = group
                                    .group
                                    .map_or_else(|| "row group".into(), |group| format!("<{}>", group.el.tag));
                                return Err(Error::new(format!(
                                    "rowSpan {span} runs past the last row of its {within}; rows joined by rowSpan stay in one row group"
                                ))
                                .locate(&cell.at));
                            }
                            if columns > 1
                                && (cell.style.get(p!("width")).is_some()
                                    || cell.style.get(p!("flex")).is_some())
                            {
                                return Err(Error::new(
                                    "width and flex size one column; set them on a cell without colSpan in that column",
                                )
                                .locate(&cell.at));
                            }
                            let vertical = [&cell.style, &tr.style]
                                .into_iter()
                                .chain(group.group.map(|group| &group.style))
                                .find_map(|style| get(style, p!("verticalAlign")))
                                .and_then(|value| value.text())
                                .unwrap_or("top");
                            let Some(align) = ALIGN.iter().position(|own| *own == vertical) else {
                                return Err(Error::new(
                                    "vertical-align: baseline is not implemented in tables; use top, middle, or bottom",
                                )
                                .locate(&cell.at));
                            };
                            for c in column..column + columns {
                                if covered.get(c).is_some_and(|count| *count != 0) {
                                    return Err(Error::new(
                                        "colSpan runs into a cell that rowSpan brings down from an earlier row",
                                    )
                                    .locate(&cell.at));
                                }
                                if covered.len() <= c {
                                    covered.resize(c + 1, 0);
                                }
                                covered[c] = i64::from(span);
                                row.slots.push(if c == column {
                                    Slot::Cell {
                                        align: align as u8,
                                        rows: span,
                                    }
                                } else {
                                    Slot::Join
                                });
                            }
                            row.cells.push(GridCell {
                                node: cell,
                                row: rows.len(),
                                column,
                                columns,
                                rows: usize::from(span),
                            });
                            reach = reach.max(index + usize::from(span));
                        }
                        skip(&mut row, &covered);
                        if row.slots.is_empty() {
                            return Err(Error::new("<tr> has no cells"));
                        }
                        if width == 0 {
                            width = row.slots.len();
                        }
                        if row.slots.len() != width {
                            return Err(Error::new(format!(
                                "<tr> has {} cells but the table has {width} columns",
                                row.slots.len()
                            )));
                        }
                        for count in &mut covered {
                            *count -= 1;
                        }
                        Ok(row)
                    })()
                    .map_err(|error| error.locate(&tr.at))?;
                    indices.push(rows.len());
                    rows.push(row);
                }
                placed[index].push(GridGroup {
                    group: group.group,
                    rows: indices,
                });
            }
        }
        if rows.is_empty() {
            return Err(Error::new("<table> has no rows"));
        }
        // Each column takes its width from its first cell without colSpan.
        let mut columns = Vec::with_capacity(width);
        for index in 0..width {
            let cell = rows
                .iter()
                .flat_map(|row| &row.cells)
                .find(|cell| cell.column == index && cell.columns == 1);
            columns.push(match cell {
                Some(cell) => element_column(cell.node)?,
                None => Column {
                    kind: 1,
                    value: 1.0,
                },
            });
        }
        let paints = if collapsed {
            let mut boxes = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut line = Vec::with_capacity(row.cells.len());
                for cell in &row.cells {
                    let paint = inherit(
                        &cell.node.style,
                        &row.state,
                        &self.fonts,
                        &mut self.writer.sink,
                        cell.node.el.dir,
                    )
                    .and_then(|own| Ok(box_style(&cell.node.style, own.size, 1.0)?))
                    .map_err(|error| error.locate(&cell.node.at))?;
                    line.push(paint);
                }
                boxes.push(line);
            }
            Some(collapse(&rows, width, &boxes))
        } else {
            None
        };
        // A parent flex row already consumed `width` as this table's track.
        let track = if context.in_row {
            Column {
                kind: 1,
                value: 1.0,
            }
        } else {
            element_column(node)?
        };
        if let Some(top) = paint
            .as_ref()
            .map(|paint| paint.margin[0])
            .filter(|top| *top != 0.0)
        {
            self.writer.spacer(top)?;
        }
        // Element path of each table child, indexed as the renderer reports an overflowing row.
        let mut wheres: Vec<(Rc<str>, bool)> = Vec::new();
        let avoid = is(s, p!("breakInside"), "avoid");
        let [heads, bodies, feet] = placed;
        let table = Table {
            rows: &rows,
            columns: &columns,
            paints: paints.as_deref(),
            collapsed,
            depth,
        };
        let result: Result<()> = (|| {
            if avoid {
                self.writer.stack_start(0.0, false)?;
            }
            let count = |groups: &[GridGroup]| {
                groups
                    .iter()
                    .map(|group| group.rows.iter().filter(|&&row| !rows[row].joined).count())
                    .sum::<usize>()
            };
            self.writer
                .table_start(count(&heads), count(&feet), track)?;
            // Footer rows precede the body so the engine can reserve them and place each body row on arrival.
            for group in &heads {
                self.group(&table, group, true, &mut wheres)?;
            }
            for group in &feet {
                self.group(&table, group, true, &mut wheres)?;
            }
            for group in &bodies {
                self.group(&table, group, false, &mut wheres)?;
            }
            self.writer.table_end()?;
            if avoid {
                self.writer.stack_end(false)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            // A row that overflows as it closes inside `row()` already carries its path.
            let overflow = error
                .message
                .strip_prefix("TableRowOverflow(")
                .and_then(|rest| {
                    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
                    (digits > 0 && rest[digits..].starts_with(')'))
                        .then(|| rest[..digits].parse::<usize>().ok())
                });
            if let Some(index) = overflow {
                let child = index.and_then(|index| wheres.get(index));
                let message = if child.is_some_and(|(_, joined)| *joined) {
                    "rows that rowSpan joins are taller than the page beside the repeated header and footer; shorten the spanning cell or split the rows"
                } else {
                    "a table row is taller than the page beside its repeated header and footer; split its content"
                };
                return Err(Error::new(message).locate(child.map_or(&node.at, |(at, _)| at)));
            }
            if error.message == "PageOverflow" {
                return Err(Error::new(if avoid {
                    "PageOverflow: the table is taller than a page; drop break-inside: avoid"
                } else {
                    "PageOverflow: the table header and footer are taller than a page"
                })
                .locate(&node.at));
            }
            return Err(error);
        }
        if let Some(bottom) = paint
            .map(|paint| paint.margin[2])
            .filter(|bottom| *bottom != 0.0)
        {
            self.writer.margin(bottom);
        }
        self.page_break(s, true, depth)
    }

    fn group(
        &mut self,
        table: &Table,
        group: &GridGroup,
        repeated: bool,
        wheres: &mut Vec<(Rc<str>, bool)>,
    ) -> Result<()> {
        let structure = group
            .rows
            .first()
            .map_or(0, |row| table.rows[*row].state.structure);
        let structure = if structure == 0 {
            0
        } else {
            self.structure_node(
                match group.group.map(|group| group.el.tag.as_str()) {
                    Some("thead") => "THead",
                    Some("tfoot") => "TFoot",
                    _ => "TBody",
                },
                structure,
            )
        };
        if structure != 0 {
            self.structure[structure as usize - 1].lang =
                group.group.and_then(|group| group.el.lang.clone());
        }
        let style = group.group.map(|group| &*group.style);
        let says = |prop, value| style.is_some_and(|style| is(style, prop, value));
        if !repeated && says(p!("breakBefore"), "avoid") {
            self.writer.avoid()?;
        }
        // Header and footer rows always share a page, so `break-inside: avoid` only groups body rows.
        let avoid = says(p!("breakInside"), "avoid") && !repeated;
        // Each path goes in first: the engine rejects an overflowing child the moment it closes.
        if avoid {
            wheres.push((group.group.unwrap().at.clone(), false));
            self.writer.stack_start(0.0, false)?;
        }
        let rows = &group.rows;
        let mut start = 0;
        while start < rows.len() {
            let mut end = start + 1;
            while rows.get(end).is_some_and(|&row| table.rows[row].joined) {
                end += 1;
            }
            // Rows that rowSpan joins move between pages together, like break-inside: avoid.
            let joined = end - start > 1;
            if !avoid {
                wheres.push((table.rows[rows[start]].node.at.clone(), joined));
            }
            if joined {
                self.writer.stack_start(0.0, false)?;
            }
            for &row in &rows[start..end] {
                self.row(table, row, joined, structure)?;
            }
            if joined {
                self.writer.stack_end(false)?;
            }
            start = end;
        }
        if avoid {
            self.writer.stack_end(false)?;
        }
        if !repeated && says(p!("breakAfter"), "avoid") {
            self.writer.avoid()?;
        }
        Ok(())
    }

    fn row(&mut self, table: &Table, index: usize, joined: bool, parent: u32) -> Result<()> {
        let item = &table.rows[index];
        let structure = if parent == 0 {
            0
        } else {
            self.structure_node("TR", parent)
        };
        if structure != 0 {
            self.structure[structure as usize - 1].lang = item.node.el.lang.clone();
        }
        (|| {
            if is(&item.node.style, p!("breakBefore"), "avoid") {
                self.writer.avoid()?;
            }
            // Collapsed borders belong to the cells.
            let own = item.paint.clone().map(|paint| {
                if table.collapsed {
                    Box {
                        border: [0.0; 4],
                        ..paint
                    }
                } else {
                    paint
                }
            });
            let shaped = own.as_ref().is_some_and(|own| {
                own.margin.iter().chain(&own.padding).chain(&own.border).any(|edge| *edge != 0.0)
            });
            if joined && shaped {
                return Err(Error::new(
                    "a <tr> that rowSpan joins to other rows takes only a background; style its cells",
                ));
            }
            let background = own.as_ref().and_then(|own| own.background);
            // Joined rows lay out as one block, so each row's background paints in its own cells.
            let value = if joined || !(shaped || background.is_some()) {
                None
            } else {
                own
            };
            self.boxed(value, false, false, |this| {
                this.writer.row_start(table.columns.to_vec(), item.slots.clone())?;
                for (i, cell) in item.cells.iter().enumerate() {
                    let mut paint = table
                        .paints
                        .map_or_else(CellPaint::default, |paints| paints[index][i]);
                    if joined {
                        paint.background = background;
                    }
                    let context = Ctx {
                        structure,
                        in_row: true,
                        cell: Some(paint),
                        ..item.state.clone()
                    };
                    this.compile_element(cell.node, &context, table.depth + 2)?;
                }
                this.writer.row_end()
            })?;
            if is(&item.node.style, p!("breakAfter"), "avoid") {
                self.writer.avoid()?;
            }
            Ok(())
        })()
        .map_err(|error| error.locate(&item.node.at))
    }
}

struct Table<'t, 's, 'a> {
    rows: &'t [GridRow<'s, 'a>],
    columns: &'t [Column],
    paints: Option<&'t [Vec<CellPaint>]>,
    collapsed: bool,
    depth: usize,
}
