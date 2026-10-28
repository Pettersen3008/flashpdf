use std::ops::Range;

use taffy::prelude::*;
use taffy::{compute_leaf_layout, DetailedLayoutInfo, Layout, MinMax};

use crate::document::{Element, GridNode};
use crate::geometry::LayoutArea;
use crate::{GridBreadth, GridLine, GridTrack, ImageSize, Pt};

use super::{overflows, LayoutBuffer, LayoutEngine, LayoutError, TOLERANCE};

pub(super) type GridKey = (usize, usize, u8, u32, Option<u32>);

#[derive(Clone)]
pub(crate) struct GridRows {
    items: Vec<(Range<usize>, Layout, Range<usize>, f32)>,
    pub(crate) rows: Vec<GridRow>,
    height: f32,
    width: f32,
}

#[derive(Clone)]
pub(crate) struct GridRow {
    pub(crate) start: f32,
    pub(crate) end: f32,
    tracks: Range<usize>,
}

fn alignment(code: u8) -> Option<AlignItems> {
    Some(match code {
        0 => return None,
        1 => AlignItems::STRETCH,
        2 => AlignItems::START,
        3 => AlignItems::END,
        4 => AlignItems::CENTER,
        5 => AlignItems::NORMAL,
        _ => unreachable!(),
    })
}

fn content_alignment(code: u8) -> AlignContent {
    match code {
        0 => AlignContent::STRETCH,
        1 => AlignContent::START,
        2 => AlignContent::END,
        3 => AlignContent::CENTER,
        4 => AlignContent::SPACE_BETWEEN,
        5 => AlignContent::SPACE_AROUND,
        6 => AlignContent::SPACE_EVENLY,
        _ => unreachable!(),
    }
}

fn placement(lines: [GridLine; 2]) -> Line<GridPlacement> {
    let [start, end] = lines.map(|value| match value {
        GridLine::Auto => auto(),
        GridLine::Line(value) => line(value),
        GridLine::Span(value) => span(value),
    });
    Line { start, end }
}

fn track(value: &GridTrack) -> TrackSizingFunction {
    let min = match value.min {
        GridBreadth::Length(value) => length(value),
        GridBreadth::Percent(value) => percent(value),
        GridBreadth::Auto => auto(),
        GridBreadth::MinContent => min_content(),
        GridBreadth::MaxContent => max_content(),
        GridBreadth::Fr(_) => unreachable!(),
    };
    let max = match value.max {
        GridBreadth::Length(value) => length(value),
        GridBreadth::Percent(value) => percent(value),
        GridBreadth::Fr(value) => fr(value),
        GridBreadth::Auto => auto(),
        GridBreadth::MinContent => min_content(),
        GridBreadth::MaxContent => max_content(),
    };
    MinMax { min, max }
}

fn dimension<T: FromLength + FromPercent + TaffyAuto>(value: ImageSize) -> T {
    match value {
        ImageSize::Auto => auto(),
        ImageSize::Fixed(value) => length(value.get()),
        ImageSize::Percent(value) => percent(value.get() / 100.0),
    }
}

fn item_style(element: &Element<'_>) -> Style {
    let mut style = Style::default();
    match element {
        Element::Box(node) => {
            let size = node.size;
            let edges = node.style;
            style.box_sizing = if size.border_box {
                BoxSizing::BorderBox
            } else {
                BoxSizing::ContentBox
            };
            style.size = Size {
                width: dimension(size.width),
                height: size.height.map_or(auto(), |v| length(v.get())),
            };
            style.min_size = Size {
                width: dimension(size.min_width),
                height: length(size.min_height.get()),
            };
            style.max_size = Size {
                width: dimension(size.max_width),
                height: size.max_height.map_or(auto(), |v| length(v.get())),
            };
            let margin = |value: Pt, bit| {
                if size.auto_margin & bit != 0 {
                    auto()
                } else {
                    length(value.get())
                }
            };
            style.margin = Rect {
                top: margin(edges.margin.top, 1),
                right: margin(edges.margin.right, 2),
                bottom: margin(edges.margin.bottom, 4),
                left: margin(edges.margin.left, 8),
            };
            style.padding = Rect {
                top: length(edges.padding.top.get()),
                right: length(edges.padding.right.get()),
                bottom: length(edges.padding.bottom.get()),
                left: length(edges.padding.left.get()),
            };
            style.border = Rect {
                top: length(edges.border.top.get()),
                right: length(edges.border.right.get()),
                bottom: length(edges.border.bottom.get()),
                left: length(edges.border.left.get()),
            };
        }
        Element::Image(node) => {
            style.size = Size {
                width: dimension(node.width),
                height: node.height.map_or(auto(), |v| length(v.get())),
            };
            style.max_size = Size {
                width: dimension(node.max_width),
                height: node.max_height.map_or(auto(), |v| length(v.get())),
            };
            style.item_is_replaced = true;
        }
        _ => {}
    }
    style
}

impl LayoutEngine<'_> {
    pub(crate) fn grid_rows(
        &self,
        node: &GridNode<'_>,
        width: Pt,
    ) -> Result<GridRows, LayoutError> {
        self.grid_sizing(node, AvailableSpace::Definite(width.get()), None)
    }

    pub(super) fn grid_intrinsic(&self, node: &GridNode<'_>) -> Result<(f32, f32), LayoutError> {
        Ok((
            self.grid_sizing(node, AvailableSpace::MinContent, None)?
                .width,
            self.grid_sizing(node, AvailableSpace::MaxContent, None)?
                .width,
        ))
    }

    fn grid_sizing(
        &self,
        node: &GridNode<'_>,
        available_width: AvailableSpace,
        assigned_height: Option<f32>,
    ) -> Result<GridRows, LayoutError> {
        let constraint = match available_width {
            AvailableSpace::Definite(width) => (0, width.to_bits()),
            AvailableSpace::MinContent => (1, 0),
            AvailableSpace::MaxContent => (2, 0),
        };
        let key = (
            node.style as *const _ as usize,
            node.children.as_ptr() as usize,
            constraint.0,
            constraint.1,
            assigned_height.map(f32::to_bits),
        );
        if let Some(rows) = self.grids.borrow().get(&key) {
            return Ok(rows.clone());
        }
        let grid = node.style;
        grid.validate().map_err(|_| LayoutError::InvalidBox)?;
        // Bound the worst-case implicit placement before Taffy allocates its occupancy matrix.
        for (explicit, vertical) in [(grid.columns.len(), false), (grid.rows.len(), true)] {
            let upper = node
                .items
                .iter()
                .map(|item| {
                    let lines = if vertical { item.row } else { item.column };
                    match lines {
                        [GridLine::Line(a), GridLine::Line(b)] => usize::from(a.abs_diff(b)).max(1),
                        [GridLine::Span(n), _] | [_, GridLine::Span(n)] => usize::from(n),
                        _ => 1,
                    }
                })
                .sum::<usize>()
                + explicit
                + 512;
            if upper > 4096 {
                return Err(LayoutError::InvalidBox);
            }
        }
        let measure = self;
        let mut tree: TaffyTree<usize> = TaffyTree::new();
        tree.disable_rounding();
        let mut leaves = Vec::with_capacity(node.items.len());
        let mut sources = Vec::with_capacity(node.items.len());
        let mut intrinsic = Vec::with_capacity(node.items.len());
        let mut start = 0;
        for (index, child) in node.children.iter().enumerate() {
            if matches!(child, Element::Anchor(_)) {
                continue;
            }
            let spec = &node.items[leaves.len()];
            let mut style = item_style(child);
            if let Element::Image(node) = child {
                let image = self
                    .images
                    .get(usize::from(node.slot))
                    .ok_or(LayoutError::UnknownImage)?;
                style.aspect_ratio = Some(image.width as f32 / image.height as f32);
            }
            style.grid_column = placement(spec.column);
            style.grid_row = placement(spec.row);
            style.justify_self = alignment(spec.justify);
            style.align_self = alignment(spec.align);
            intrinsic.push(match child {
                Element::Box(node) => measure.contents(&node.children)?,
                _ => measure.intrinsic(child)?,
            });
            sources.push(start..index + 1);
            start = index + 1;
            leaves.push(
                tree.new_leaf_with_context(style, leaves.len())
                    .map_err(|_| LayoutError::InvalidBox)?,
            );
        }
        let hidden = if leaves.is_empty() {
            vec![tree
                .new_leaf(Style {
                    display: Display::None,
                    ..Style::default()
                })
                .map_err(|_| LayoutError::InvalidBox)?]
        } else {
            Vec::new()
        };
        let root = tree
            .new_with_children(
                Style {
                    display: Display::Grid,
                    size: Size {
                        width: match available_width {
                            AvailableSpace::Definite(width) => length(width),
                            _ => auto(),
                        },
                        height: if let Some(height) = assigned_height {
                            length(height)
                        } else if grid.max_height == Some(grid.min_height) {
                            length(grid.min_height)
                        } else {
                            auto()
                        },
                    },
                    min_size: Size {
                        width: auto(),
                        height: length(grid.min_height),
                    },
                    max_size: Size {
                        width: auto(),
                        height: grid.max_height.map_or(auto(), length),
                    },
                    grid_template_columns: grid
                        .columns
                        .iter()
                        .map(|v| GridTemplateComponent::Single(track(v)))
                        .collect(),
                    grid_template_rows: grid
                        .rows
                        .iter()
                        .map(|v| GridTemplateComponent::Single(track(v)))
                        .collect(),
                    grid_auto_columns: grid.auto_columns.iter().map(track).collect(),
                    grid_auto_rows: grid.auto_rows.iter().map(track).collect(),
                    grid_auto_flow: if grid.column_flow {
                        GridAutoFlow::Column
                    } else {
                        GridAutoFlow::Row
                    },
                    gap: Size {
                        width: length(grid.column_gap),
                        height: length(grid.row_gap),
                    },
                    justify_items: alignment(grid.justify_items).unwrap_or(AlignItems::NORMAL),
                    align_items: alignment(grid.align_items).unwrap_or(AlignItems::NORMAL),
                    justify_content: content_alignment(grid.justify_content),
                    align_content: content_alignment(grid.align_content),
                    ..Style::default()
                },
                if leaves.is_empty() { &hidden } else { &leaves },
            )
            .map_err(|_| LayoutError::InvalidBox)?;
        let mut failure = None;
        tree.compute_layout_with_measure(
            root,
            Size {
                width: available_width,
                height: AvailableSpace::MaxContent,
            },
            |inputs, _, context, style| {
                compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known, available| {
                        let Some(&mut item) = context else {
                            return Size::ZERO;
                        };
                        let index = sources[item].end - 1;
                        let (min, max) = intrinsic[item];
                        let (inset_x, inset_y) = match &node.children[index] {
                            Element::Box(node) => (
                                (node.style.padding.left
                                    + node.style.padding.right
                                    + node.style.border.left
                                    + node.style.border.right)
                                    .get(),
                                (node.style.padding.top
                                    + node.style.padding.bottom
                                    + node.style.border.top
                                    + node.style.border.bottom)
                                    .get(),
                            ),
                            _ => (0.0, 0.0),
                        };
                        let width = known
                            .width
                            .map(|width| width - inset_x)
                            .unwrap_or(match available.width {
                                AvailableSpace::MinContent => min,
                                AvailableSpace::MaxContent => max,
                                AvailableSpace::Definite(width) => width.min(max),
                            })
                            .max(0.0);
                        let mut scratch = LayoutBuffer::default();
                        let area = LayoutArea::root(Pt(width.max(TOLERANCE)));
                        let before = measure.absolutes.get();
                        let result = match &node.children[index] {
                            Element::Box(node) => {
                                measure.layout_rows(&node.children, Pt::ZERO, area, &mut scratch)
                            }
                            Element::Image(node) => measure.layout_image(
                                &crate::document::ImageNode {
                                    width: ImageSize::Fixed(Pt(width)),
                                    max_width: ImageSize::Auto,
                                    ..*node
                                },
                                area,
                                &mut scratch,
                            ),
                            child => measure.layout(child, area, &mut scratch),
                        };
                        measure.absolutes.set(before);
                        let height = match result {
                            Ok(height) => height.get(),
                            Err(
                                LayoutError::InvalidWidth
                                | LayoutError::InvalidBox
                                | LayoutError::TextTooWide
                                | LayoutError::LineTooWide
                                | LayoutError::ImageTooWide
                                | LayoutError::ContentOverflow,
                            ) => 0.0,
                            Err(error) => {
                                failure.get_or_insert(error);
                                0.0
                            }
                        };
                        Size {
                            width,
                            height: known
                                .height
                                .map_or(height, |height| (height - inset_y).max(0.0)),
                        }
                    },
                )
            },
        )
        .map_err(|_| LayoutError::InvalidBox)?;
        if let Some(error) = failure {
            return Err(error);
        }
        let DetailedLayoutInfo::Grid(info) = tree.detailed_layout_info(root) else {
            return Err(LayoutError::InvalidBox);
        };
        let container = tree.layout(root).map_err(|_| LayoutError::InvalidBox)?.size;
        let (width, height) = (container.width, container.height);
        let tracks = &info.rows.positions;
        if tracks.len() > 4096 || info.columns.positions.len() > 4096 {
            return Err(LayoutError::InvalidBox);
        }
        let mut items = Vec::with_capacity(leaves.len());
        let mut joined = vec![false; tracks.len()];
        for ((leaf, source), placement) in leaves.iter().zip(sources).zip(&info.items) {
            let rows = usize::from(placement.row_start - 1)..usize::from(placement.row_end - 1);
            joined[rows.start + 1..rows.end].fill(true);
            let layout = *tree.layout(*leaf).map_err(|_| LayoutError::InvalidBox)?;
            if matches!(available_width, AvailableSpace::Definite(_))
                && (![
                    layout.location.x,
                    layout.location.y,
                    layout.size.width,
                    layout.size.height,
                ]
                .iter()
                .all(|v| v.is_finite())
                    || layout.location.x < -TOLERANCE
                    || overflows(
                        width - (layout.location.x + layout.size.width + layout.margin.right),
                    )
                    || layout.location.y < -TOLERANCE
                    || overflows(
                        height - (layout.location.y + layout.size.height + layout.margin.bottom),
                    ))
            {
                return Err(LayoutError::ContentOverflow);
            }
            let columns =
                usize::from(placement.column_start - 1)..usize::from(placement.column_end - 1);
            let area_width = info.columns.positions[columns.end - 1].end
                - info.columns.positions[columns.start].start;
            items.push((source, layout, rows, area_width));
        }
        if tracks
            .iter()
            .any(|v| !v.start.is_finite() || !v.end.is_finite())
            || tracks.last().is_some_and(|v| overflows(height - v.end))
        {
            return Err(LayoutError::ContentOverflow);
        }
        let mut rows = Vec::new();
        let mut first = 0;
        for end in 1..=tracks.len() {
            if end == tracks.len() || !joined[end] {
                rows.push(GridRow {
                    start: tracks[first].start,
                    end: tracks[end - 1].end,
                    tracks: first..end,
                });
                first = end;
            }
        }
        let result = GridRows {
            items,
            rows,
            height,
            width,
        };
        self.grids.borrow_mut().insert(key, result.clone());
        Ok(result)
    }

    pub(super) fn layout_grid(
        &self,
        node: &GridNode<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        self.layout_grid_height(node, area, None, output)
    }

    pub(super) fn layout_grid_height(
        &self,
        node: &GridNode<'_>,
        area: LayoutArea,
        height: Option<f32>,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let rows = self.grid_sizing(node, AvailableSpace::Definite(area.width.get()), height)?;
        let layer_start = output.layers.len();
        for row in &rows.rows {
            self.layout_grid_row(
                node,
                &rows,
                row,
                area.translated(Pt::ZERO, Pt(row.start)),
                output,
                0,
            )?;
        }
        output.layers[layer_start..]
            .sort_by_key(|layer| (layer.z, layer.order.map_or(usize::MAX, |v| v.1)));
        for layer in &mut output.layers[layer_start..] {
            layer.order = None;
        }
        if rows.rows.is_empty() {
            for child in &node.children {
                self.layout(child, area, output)?;
            }
        }
        Ok(Pt(rows.height))
    }

    pub(crate) fn layout_grid_root_row(
        &self,
        node: &GridNode<'_>,
        rows: &GridRows,
        row: &GridRow,
        area: LayoutArea,
        output: &mut LayoutBuffer,
        group: u32,
    ) -> Result<Pt, LayoutError> {
        let before = self.absolutes.get();
        let height = self.layout_grid_row(node, rows, row, area, output, group)?;
        if self.absolutes.get() != before {
            for (source, _, tracks, _) in &rows.items {
                if row.tracks.contains(&tracks.start) {
                    self.place_absolutes(&node.children[source.clone()], None, output)?;
                }
            }
        }
        Ok(height)
    }

    pub(crate) fn layout_grid_row(
        &self,
        node: &GridNode<'_>,
        rows: &GridRows,
        row: &GridRow,
        area: LayoutArea,
        output: &mut LayoutBuffer,
        group: u32,
    ) -> Result<Pt, LayoutError> {
        for (source, rect, tracks, area_width) in &rows.items {
            if !row.tracks.contains(&tracks.start) {
                continue;
            }
            let destination = &mut *output;
            let mut item_output = LayoutBuffer::default();
            let output = &mut item_output;
            let element = &node.children[source.end - 1];
            let area = area
                .translated(Pt(rect.location.x), Pt(rect.location.y - row.start))
                .with_width(Pt(rect.size.width.max(TOLERANCE)));
            for anchor in &node.children[source.start..source.end - 1] {
                self.layout(anchor, area, output)?;
            }
            let height = match element {
                Element::Box(node) => {
                    let mut style = node.style;
                    style.margin = crate::Edges::all(Pt::ZERO);
                    let size = crate::BoxSize {
                        width: ImageSize::Fixed(Pt(rect.size.width)),
                        height: Some(Pt(rect.size.height)),
                        border_box: true,
                        ..crate::BoxSize::AUTO
                    };
                    match node.size.position {
                        Some(position) => self
                            .positioned(
                                node,
                                position,
                                (size, style),
                                area.with_width(Pt(*area_width)),
                                Some(Pt(rect.size.width)),
                                output,
                            )?
                            .get(),
                        None => self
                            .block(node, (size, style), area, Some(Pt(rect.size.width)), output)?
                            .0
                            .get(),
                    }
                }
                Element::Image(node) => self
                    .layout_image(
                        &crate::document::ImageNode {
                            width: ImageSize::Fixed(Pt(rect.size.width)),
                            height: Some(Pt(rect.size.height)),
                            max_width: ImageSize::Auto,
                            max_height: None,
                            ..*node
                        },
                        area,
                        output,
                    )?
                    .get(),
                child => self.layout(child, area, output)?.get(),
            };
            if overflows(rect.size.height - height) {
                return Err(LayoutError::ContentOverflow);
            }
            let marker = destination.lines.len();
            destination.lines.push(super::output::PositionedLine {
                segments: destination.segments.len()..destination.segments.len(),
                origin: area.origin,
                top: area.origin.y,
                bottom: area.origin.y + Pt(rect.size.height),
                available_width: area.width,
                text_width: Pt::ZERO,
                align: crate::TextAlign::Left,
            });
            let first = destination.layers.len();
            super::position::push_layer(
                destination,
                None,
                crate::geometry::Point::ZERO,
                false,
                item_output,
            );
            destination.layers[first].at = Some(marker);
            for layer in &mut destination.layers[first..] {
                layer.order = Some((group, source.end));
            }
        }
        if rows
            .rows
            .last()
            .is_some_and(|last| last.tracks == row.tracks)
        {
            for child in node
                .children
                .iter()
                .skip(rows.items.last().map_or(0, |v| v.0.end))
            {
                self.layout(
                    child,
                    area.translated(Pt::ZERO, Pt(row.end - row.start)),
                    output,
                )?;
            }
        }
        Ok(Pt(row.end - row.start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandParser;
    use crate::css::{style::Style as CssStyle, Key, Prop};
    use crate::document::Block;
    use crate::{BoxSize, BoxStyle, Command, Edges, GridItem, LineStyle, Page, Rgb, TextStyle};

    const ITEM: GridItem = GridItem {
        column: [GridLine::Auto; 2],
        row: [GridLine::Auto; 2],
        justify: 0,
        align: 0,
    };

    fn grid(columns: &str, rows: &str) -> crate::Grid {
        let s = CssStyle(vec![
            (Key::Prop(Prop::of("gridTemplateColumns")), columns.into()),
            (Key::Prop(Prop::of("gridTemplateRows")), rows.into()),
        ]);
        crate::css::style::grid::container(&s, 12.0).unwrap()
    }

    fn layout(
        grid: &crate::Grid,
        items: &[GridItem],
        body: &[Command<'_>],
        width: f32,
    ) -> LayoutBuffer {
        let mut commands = vec![Command::GridStart { style: grid, items }];
        commands.extend_from_slice(body);
        commands.push(Command::GridEnd);
        let Block::Element(element) = CommandParser::new(commands.as_slice())
            .parse_single_block()
            .unwrap()
        else {
            panic!()
        };
        let fonts = crate::font::FontBook::new(vec![]);
        let mut output = LayoutBuffer::default();
        LayoutEngine::new(&fonts, &[], Page::A4)
            .layout_root(&element, LayoutArea::root(Pt(width)), &mut output)
            .unwrap();
        fn gather(output: &mut LayoutBuffer) {
            output.lines.retain(|line| !line.segments.is_empty());
            let layers = std::mem::take(&mut output.layers);
            for mut layer in layers {
                gather(&mut layer.buffer);
                output.lines.extend(layer.buffer.lines);
                output.boxes.extend(layer.buffer.boxes);
            }
        }
        gather(&mut output);
        output
    }

    #[test]
    fn given_fractional_tracks_and_a_gap_when_laying_out_then_text_starts_at_taffys_track_positions(
    ) {
        let mut grid = grid("20pt minmax(0, 1fr) 2fr", "24pt");
        grid.column_gap = 5.0;
        let body = ["a", "b", "c"].map(|text| Command::Text {
            text,
            style: TextStyle::plain(Pt(8.0)),
        });
        let output = layout(&grid, &[ITEM; 3], &body, 200.0);
        for (line, x) in output.lines.iter().zip([0.0, 25.0, 86.66667]) {
            assert!(
                (line.origin.x.get() - x).abs() < TOLERANCE,
                "{} != {x}",
                line.origin.x.get()
            );
        }
    }

    #[test]
    fn given_a_padded_item_with_margins_when_aligned_then_its_border_box_and_text_share_the_assigned_frame(
    ) {
        let grid = grid("100pt", "40pt");
        let style = BoxStyle {
            margin: Edges {
                top: Pt(3.0),
                right: Pt(4.0),
                bottom: Pt(3.0),
                left: Pt(4.0),
            },
            padding: Edges::all(Pt(2.0)),
            border: Edges::all(Pt(1.0)),
            background: Some(Rgb::BLACK),
            border_color: Edges::all(Rgb::BLACK),
            border_style: Edges::all(LineStyle::Solid),
            radius: None,
            alpha: 255,
        };
        let body = [
            Command::BoxStart {
                style,
                size: BoxSize {
                    width: ImageSize::Fixed(Pt(20.0)),
                    height: Some(Pt(20.0)),
                    ..BoxSize::AUTO
                },
            },
            Command::Text {
                text: "a",
                style: TextStyle::plain(Pt(8.0)),
            },
            Command::BoxEnd,
        ];
        let output = layout(
            &grid,
            &[GridItem {
                justify: 4,
                align: 3,
                ..ITEM
            }],
            &body,
            100.0,
        );
        let painted = output.boxes[0];
        assert_eq!(
            (
                painted.origin.x.get(),
                painted.origin.y.get(),
                painted.width.get(),
                painted.height.get()
            ),
            (37.0, 11.0, 26.0, 26.0)
        );
        assert_eq!(output.lines[0].origin.x.get(), 40.0);
        assert_eq!(output.lines[0].top.get(), 14.0);
    }

    #[test]
    fn given_transitive_row_spans_when_resolving_breaks_then_the_joined_rows_form_one_group() {
        let grid = grid("repeat(2, 30pt)", "repeat(4, 20pt)");
        let at = |column, start, end| GridItem {
            column: [GridLine::Line(column), GridLine::Auto],
            row: [GridLine::Line(start), GridLine::Line(end)],
            ..ITEM
        };
        let items = [at(1, 1, 3), at(2, 2, 4), at(1, 4, 5)];
        let node = GridNode {
            style: &grid,
            items: &items,
            children: (0..3)
                .map(|_| Element::Spacer(crate::document::Spacer { height: Pt(10.0) }))
                .collect(),
        };
        let fonts = crate::font::FontBook::new(vec![]);
        let rows = LayoutEngine::new(&fonts, &[], Page::A4)
            .grid_rows(&node, Pt(60.0))
            .unwrap();
        assert_eq!(
            rows.rows
                .iter()
                .map(|r| (r.start, r.end))
                .collect::<Vec<_>>(),
            vec![(0.0, 60.0), (60.0, 80.0)]
        );
    }

    #[test]
    fn given_an_empty_grid_with_explicit_tracks_when_laying_out_then_it_keeps_track_height_without_creating_an_item(
    ) {
        let grid = grid("30pt", "10pt 20pt");
        let node = GridNode {
            style: &grid,
            items: &[],
            children: vec![],
        };
        let fonts = crate::font::FontBook::new(vec![]);
        let rows = LayoutEngine::new(&fonts, &[], Page::A4)
            .grid_rows(&node, Pt(60.0))
            .unwrap();
        assert_eq!(rows.height, 30.0);
        assert_eq!(rows.rows.len(), 2);
        assert!(rows.items.is_empty());
    }
    #[test]
    fn given_padded_text_near_a_wrap_boundary_when_measured_then_the_row_fits_both_lines() {
        let grid = grid("80pt", "auto");
        let style = BoxStyle {
            margin: Edges::all(Pt::ZERO),
            padding: Edges::all(Pt(10.0)),
            border: Edges::all(Pt::ZERO),
            background: Some(Rgb::BLACK),
            border_color: Edges::all(Rgb::BLACK),
            border_style: Edges::all(LineStyle::Solid),
            radius: None,
            alpha: 255,
        };
        let body = [
            Command::BoxStart {
                style,
                size: BoxSize::AUTO,
            },
            Command::Text {
                text: "12345 12345",
                style: TextStyle {
                    font: crate::COURIER,
                    ..TextStyle::plain(Pt(10.0))
                },
            },
            Command::BoxEnd,
        ];
        let output = layout(&grid, &[ITEM], &body, 80.0);
        assert_eq!(output.lines.len(), 2);
        assert_eq!(output.lines[0].available_width.get(), 60.0);
        assert!(output.boxes[0].height.get() > 40.0);
    }

    #[test]
    fn given_deeply_nested_flexible_grids_when_laying_out_then_sizing_reuses_each_constraint() {
        let grid = grid("minmax(0,1fr)", "auto");
        let mut commands = Vec::new();
        for _ in 0..20 {
            commands.push(Command::GridStart {
                style: &grid,
                items: &[ITEM],
            });
        }
        commands.push(Command::Text {
            text: "nested",
            style: TextStyle::plain(Pt(8.0)),
        });
        for _ in 0..20 {
            commands.push(Command::GridEnd);
        }
        let Block::Element(element) = CommandParser::new(commands.as_slice())
            .parse_single_block()
            .unwrap()
        else {
            panic!()
        };
        let fonts = crate::font::FontBook::new(vec![]);
        let engine = LayoutEngine::new(&fonts, &[], Page::A4);
        let mut output = LayoutBuffer::default();
        engine
            .layout_root(&element, LayoutArea::root(Pt(100.0)), &mut output)
            .unwrap();
        assert!(!output.layers.is_empty());
        assert!(
            engine.grids.borrow().len() <= 100,
            "{} sizing entries",
            engine.grids.borrow().len()
        );
    }
    #[test]
    fn given_an_image_grid_item_when_aligning_then_inherits_the_container_and_normal_keeps_its_intrinsic_size(
    ) {
        let image = crate::image::Image {
            width: 40,
            height: 20,
            color_space: crate::image::ColorSpace::Gray,
            encoding: crate::image::Encoding::Flate,
            data: vec![],
            alpha: None,
            color_key: None,
        };
        let images = [image];
        let fonts = crate::font::FontBook::new(vec![]);
        for (justify, align, expected) in [
            (5, 5, (0.0, 0.0, 30.0, 15.0)),
            (3, 4, (70.0, 17.5, 30.0, 15.0)),
            (1, 1, (0.0, 0.0, 100.0, 50.0)),
        ] {
            let mut grid = grid("100pt", "50pt");
            grid.justify_items = justify;
            grid.align_items = align;
            let node = GridNode {
                style: &grid,
                items: &[ITEM],
                children: vec![Element::Image(crate::document::ImageNode {
                    slot: 0,
                    width: ImageSize::Auto,
                    height: None,
                    max_width: ImageSize::Auto,
                    max_height: None,
                    link: None,
                    alt: "",
                    alpha: 255,
                })],
            };
            let engine = LayoutEngine::new(&fonts, &images, Page::A4);
            let rows = engine.grid_rows(&node, Pt(100.0)).unwrap();
            let rect = rows.items[0].1;
            assert_eq!(
                (
                    rect.location.x,
                    rect.location.y,
                    rect.size.width,
                    rect.size.height
                ),
                expected
            );
        }
    }
    #[test]
    fn given_a_percentage_image_width_and_cap_when_painting_then_the_grid_area_resolves_each_once()
    {
        let image = crate::image::Image {
            width: 40,
            height: 20,
            color_space: crate::image::ColorSpace::Gray,
            encoding: crate::image::Encoding::Flate,
            data: vec![],
            alpha: None,
            color_key: None,
        };
        let images = [image];
        let fonts = crate::font::FontBook::new(vec![]);
        let grid = grid("100pt", "50pt");
        let node = GridNode {
            style: &grid,
            items: &[ITEM],
            children: vec![Element::Image(crate::document::ImageNode {
                slot: 0,
                width: ImageSize::Percent(crate::Percent::new(50.0).unwrap()),
                height: None,
                max_width: ImageSize::Percent(crate::Percent::new(80.0).unwrap()),
                max_height: None,
                link: None,
                alt: "",
                alpha: 255,
            })],
        };
        let mut output = LayoutBuffer::default();
        LayoutEngine::new(&fonts, &images, Page::A4)
            .layout_grid(&node, LayoutArea::root(Pt(100.0)), &mut output)
            .unwrap();
        let painted = &output.layers[0].buffer.images[0];
        assert_eq!((painted.width.get(), painted.height.get()), (50.0, 25.0));
    }
    const FRAME: BoxStyle = BoxStyle {
        margin: Edges::all(Pt::ZERO),
        padding: Edges::all(Pt::ZERO),
        border: Edges::all(Pt::ZERO),
        background: Some(Rgb::BLACK),
        border_color: Edges::all(Rgb::BLACK),
        border_style: Edges::all(LineStyle::Solid),
        radius: None,
        alpha: 255,
    };

    #[test]
    fn given_a_stretched_nested_grid_when_laying_out_fractional_rows_then_they_share_its_assigned_height(
    ) {
        let outer = grid("100pt", "100pt");
        let mut inner = grid("100pt", "1fr 1fr");
        inner.boxed = true;
        let text = Command::Text {
            text: "a",
            style: TextStyle::plain(Pt(8.0)),
        };
        let body = [
            Command::BoxStart {
                style: FRAME,
                size: BoxSize::AUTO,
            },
            Command::GridStart {
                style: &inner,
                items: &[ITEM; 2],
            },
            Command::BoxStart {
                style: FRAME,
                size: BoxSize::AUTO,
            },
            text,
            Command::BoxEnd,
            Command::BoxStart {
                style: FRAME,
                size: BoxSize::AUTO,
            },
            text,
            Command::BoxEnd,
            Command::GridEnd,
            Command::BoxEnd,
        ];
        let output = layout(&outer, &[ITEM], &body, 100.0);
        assert_eq!(
            output
                .boxes
                .iter()
                .map(|b| (b.origin.y.get(), b.height.get()))
                .collect::<Vec<_>>(),
            vec![(0.0, 100.0), (0.0, 50.0), (50.0, 50.0)]
        );
    }

    #[test]
    fn given_a_tagged_grid_followed_by_text_in_an_atomic_box_when_painted_then_reading_order_follows_the_content(
    ) {
        let grid = grid("100pt", "20pt");
        let mut renderer = crate::Renderer::new(Page::A4);
        renderer.set_metadata(crate::pdf::Metadata {
            tagged: true,
            language: "en".into(),
            ..Default::default()
        });
        renderer
            .push(&[
                Command::BoxStart {
                    style: FRAME,
                    size: BoxSize {
                        height: Some(Pt(50.0)),
                        ..BoxSize::AUTO
                    },
                },
                Command::GridStart {
                    style: &grid,
                    items: &[ITEM],
                },
                Command::Text {
                    text: "GRID",
                    style: TextStyle::plain(Pt(8.0)),
                },
                Command::GridEnd,
                Command::Text {
                    text: "AFTER",
                    style: TextStyle::plain(Pt(8.0)),
                },
                Command::BoxEnd,
            ])
            .unwrap();
        let pdf = lopdf::Document::load_mem(&renderer.finish().unwrap()).unwrap();
        let content = String::from_utf8(pdf.get_page_content(pdf.get_pages()[&1])).unwrap();
        let grid_at = content.find("(GRID)").unwrap();
        let after = content.find("(AFTER)").unwrap();
        assert!(
            content.find("/MCID 0").unwrap() < grid_at
                && grid_at < content.find("/MCID 1").unwrap()
                && content.find("/MCID 1").unwrap() < after,
            "{content}"
        );
    }

    #[test]
    fn given_a_grid_after_an_overlapping_positioned_sibling_when_painted_then_ordinary_grid_items_paint_first(
    ) {
        let grid = grid("100pt", "20pt");
        let bytes = crate::render(
            Page::A4,
            &[
                Command::BoxStart {
                    style: BoxStyle {
                        background: Some(Rgb { r: 255, g: 0, b: 0 }),
                        ..FRAME
                    },
                    size: BoxSize {
                        width: ImageSize::Fixed(Pt(100.0)),
                        height: Some(Pt(20.0)),
                        position: Some(crate::Position {
                            kind: crate::PositionKind::Relative,
                            inset: Edges {
                                top: crate::Inset::Points(20.0),
                                ..Edges::all(crate::Inset::Auto)
                            },
                            z: None,
                        }),
                        ..BoxSize::AUTO
                    },
                },
                Command::BoxEnd,
                Command::GridStart {
                    style: &grid,
                    items: &[ITEM],
                },
                Command::BoxStart {
                    style: BoxStyle {
                        background: Some(Rgb { r: 0, g: 0, b: 255 }),
                        ..FRAME
                    },
                    size: BoxSize::AUTO,
                },
                Command::BoxEnd,
                Command::GridEnd,
            ],
        )
        .unwrap();
        let pdf = lopdf::Document::load_mem(&bytes).unwrap();
        let content = String::from_utf8(pdf.get_page_content(pdf.get_pages()[&1])).unwrap();
        assert!(
            content.find("0 0 1 rg").unwrap() < content.find("1 0 0 rg").unwrap(),
            "{content}"
        );
    }
}
