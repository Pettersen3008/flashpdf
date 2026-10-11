import type { StyledNode } from "./css.js";
import type { StyledElement } from "./css/cascade.js";
import { inherited } from "./css/properties.js";
import { glyphError } from "./fonts.js";
import { PAGE_NUMBER, TOTAL_PAGES } from "./page.js";
import type { ProtocolWriter, Column, Run, Slot } from "./protocol.js";
import {
	boxStyle,
	color,
	decoration,
	dimension,
	font,
	helvetica,
	lineHeight,
	point,
	spacing,
	type Box,
	type FontSlots,
	type LineHeight,
	type NormalizedStyle,
} from "./style.js";
import { locate, rowOnly } from "./tree.js";

type TextState = {
	size: number;
	align: "left" | "center" | "right";
	color: [number, number, number];
	family: FontSlots;
	bold: boolean;
	underline: boolean;
	lineThrough: boolean;
	lineHeight: LineHeight;
	/** The nearest enclosing `<a href>`. */
	link: string | undefined;
};
/** A cell's collapsed borders, and its row's background when rowSpan joins the row to others. */
type CellPaint = Partial<Pick<Box, "border" | "borderColor" | "background">>;
/** `cell` is set only for the direct children of a table row. */
type Context = TextState & { inRow: boolean; cell?: CellPaint };
type Fonts = ReadonlyMap<string, FontSlots>;

const ROOT: Context = {
	size: 12,
	align: "left",
	color: [0, 0, 0],
	family: helvetica,
	bold: false,
	underline: false,
	lineThrough: false,
	lineHeight: undefined,
	link: undefined,
	inRow: false,
};

const INLINE_TAGS = new Set<string>(["span", "b", "strong", "a", "br"]);
const VERTICAL_ALIGN = "vertical-align applies only to <td>, <th>, <tr>, and row groups";
const TABLE_PARTS = new Set<string>(["td", "th", "tr", "thead", "tbody", "tfoot"]);

/** CSS collapsible whitespace only; U+00A0 is content. */
function blank(text: string) {
	return /^[ \t\n\r\f]*$/.test(text);
}

/** A `br` paints nothing, so a box or layout property on it would be dropped. */
function lineBreak(node: StyledElement, context: TextState): Run {
	const key = Object.keys(node.style).find(
		(name) => !inherited.has(name) && !name.startsWith("--"),
	);
	if (key) throw locate(new Error(`<br> takes no ${key}; style the text around it`), node.where);
	return run(context, "", true);
}

const ENGINE_ERRORS = new Map([
	[
		"InvalidLayout",
		"InvalidLayout: the content does not fit its container; reduce fixed widths, margins, padding, or borders",
	],
	[
		"TextTooWide",
		"TextTooWide: a character is wider than its column; reduce the font size or widen the column",
	],
]);
/** Explains the engine's bare error codes; `locate` then names the element. */
export function engineError(error: unknown): unknown {
	const message = error instanceof Error ? ENGINE_ERRORS.get(error.message) : undefined;
	return message ? new Error(message, { cause: error }) : error;
}

function inherit(style: NormalizedStyle, current: TextState, fonts: Fonts): TextState {
	const selected = font(style, current.family, current.bold, fonts);
	const decorated = style.textDecoration === undefined ? current : decoration(style.textDecoration);
	const size =
		style.fontSize === undefined ? current.size : point(style.fontSize, current.size, true);
	return {
		size,
		align: style.textAlign ?? current.align,
		color: style.color === undefined ? current.color : color(style.color),
		family: selected.family,
		bold: selected.weight,
		underline: decorated.underline,
		lineThrough: decorated.lineThrough,
		lineHeight:
			style.lineHeight === undefined ? current.lineHeight : lineHeight(style.lineHeight, size),
		link: current.link,
	};
}

function run(state: TextState, text: string, hardBreak = false): Run {
	const height = state.lineHeight;
	return {
		font: state.bold ? state.family.bold! : state.family.regular,
		size: state.size,
		lineHeight: height === undefined ? 0 : "pt" in height ? height.pt : height.factor * state.size,
		color: state.color,
		text,
		break: hardBreak,
		underline: state.underline,
		lineThrough: state.lineThrough,
		link: state.link,
	};
}

/** Appends a run, merging it into the previous one when nothing about its paint differs. */
function push(runs: Run[], next: Run) {
	const last = runs.at(-1);
	if (
		last &&
		!last.break &&
		last.font === next.font &&
		last.size === next.size &&
		last.lineHeight === next.lineHeight &&
		last.underline === next.underline &&
		last.lineThrough === next.lineThrough &&
		last.link === next.link &&
		last.color.every((channel, index) => channel === next.color[index])
	) {
		last.text += next.text;
		last.break = next.break;
	} else runs.push(next);
}

/** Runs of an inline node; undefined for a layout node (box, page break, or its own alignment). */
function inlineRuns(node: StyledNode, context: Context, fonts: Fonts): Run[] | undefined {
	if (node.kind === "text") return [run(context, node.value)];
	if (!INLINE_TAGS.has(node.tag)) return undefined;
	if (node.tag === "br") return [lineBreak(node, context)];
	try {
		const style = node.style;
		rowOnly(style, context.inRow);
		if (style.verticalAlign !== undefined) throw new Error(VERTICAL_ALIGN);
		if (
			boxStyle(style, context.size) ||
			style.breakBefore === "page" ||
			style.breakAfter === "page"
		)
			return undefined;
		const next = inherit(style, context, fonts);
		if (next.align !== context.align) return undefined;
		return collect(node.children, { ...next, inRow: false, link: node.href ?? next.link }, fonts);
	} catch (error) {
		throw locate(error, node.where);
	}
}

/** Runs of consecutive inline children; undefined as soon as one is a layout node. */
function collect(children: readonly StyledNode[], context: Context, fonts: Fonts) {
	const runs: Run[] = [];
	for (const child of children) {
		const part = inlineRuns(child, context, fonts);
		if (part === undefined) return undefined;
		for (const item of part) push(runs, item);
	}
	return runs;
}

function writeParagraph(
	writer: ProtocolWriter,
	runs: readonly Run[],
	state: TextState,
	pageTokens: boolean,
) {
	if (
		!pageTokens &&
		runs.some(({ text }) => text.includes(PAGE_NUMBER) || text.includes(TOTAL_PAGES))
	)
		throw new Error("page tokens are only valid in a header or footer");
	writer.paragraph(runs, state.align);
}

function pageBreak(
	writer: ProtocolWriter,
	style: NormalizedStyle,
	key: "breakBefore" | "breakAfter",
	depth: number,
) {
	if (style[key] !== "page") return;
	if (depth)
		throw new Error(
			`nested ${key === "breakBefore" ? "break-before" : "break-after"} is not implemented`,
		);
	writer.pageBreak();
}

function box(writer: ProtocolWriter, value: Box | undefined, body: () => void) {
	if (value) writer.boxStart(value);
	try {
		body();
		if (value) writer.boxEnd();
	} catch (error) {
		if (value && error instanceof Error && error.message === "PageOverflow")
			throw new Error("PageOverflow: use document margins or split this decorated box");
		throw error;
	}
}

/** Block flow: consecutive inline children form one paragraph; layout nodes stand alone. */
function flow(
	children: readonly StyledNode[],
	writer: ProtocolWriter,
	fonts: Fonts,
	context: Context,
	depth: number,
	pageTokens: boolean,
	gap: number,
) {
	let count = 0;
	const separate = () => {
		if (count++ && gap) writer.spacer(gap);
	};
	let runs: Run[] = [];
	const flush = () => {
		// Whitespace between block-level children makes no line, as in CSS.
		if (runs.some((item) => item.break || !blank(item.text))) {
			separate();
			writeParagraph(writer, runs, context, pageTokens);
		}
		runs = [];
	};
	for (const child of children) {
		const part = inlineRuns(child, context, fonts);
		if (part) {
			for (const item of part) push(runs, item);
			continue;
		}
		flush();
		separate();
		compile([child], writer, fonts, context, depth, pageTokens);
	}
	flush();
}

function streamMargin(box: Box | undefined, context: Context, style: NormalizedStyle): boolean {
	return Boolean(
		box &&
		box.margin[1] === 0 &&
		box.margin[3] === 0 &&
		box.padding.every((edge) => edge === 0) &&
		box.border.every((edge) => edge === 0) &&
		box.background === undefined &&
		!context.inRow &&
		style.breakInside !== "avoid",
	);
}

function cellBox(box: Box | undefined, cell: CellPaint | undefined): Box | undefined {
	if (!cell?.border && !cell?.background) return box;
	const base = box ?? {
		margin: [0, 0, 0, 0],
		padding: [0, 0, 0, 0],
		border: [0, 0, 0, 0],
		borderColor: [BLACK, BLACK, BLACK, BLACK],
	};
	return { ...base, ...cell, background: base.background ?? cell.background };
}

function column(value: StyledNode): Column {
	if (value.kind === "text") return { kind: 1, value: 1 };
	const { flex, width } = value.style;
	if (typeof flex === "number" && flex > 0) return { kind: 1, value: flex };
	if (width !== undefined) {
		const { kind, value: amount } = dimension(width, "width");
		return { kind: kind === 2 ? 2 : 0, value: amount };
	}
	if (flex === 0)
		throw locate(
			new Error("flex: 0 (none) needs a width; columns have no content width"),
			value.where,
		);
	return { kind: 1, value: 1 };
}

function element(
	node: StyledElement,
	writer: ProtocolWriter,
	fonts: Fonts,
	context: Context,
	depth: number,
	pageTokens: boolean,
) {
	const style = node.style;
	if (style.height !== undefined && node.tag !== "img")
		throw new Error("height applies only to <img> (block height comes from content)");
	if (style.verticalAlign !== undefined && !TABLE_PARTS.has(node.tag))
		throw new Error(VERTICAL_ALIGN);
	switch (node.tag) {
		case "div":
		case "main":
		case "section":
		case "article":
		case "header":
		case "footer":
		case "td":
		case "th": {
			if ((node.tag === "td" || node.tag === "th") && !context.cell)
				throw new Error(`<${node.tag}> belongs in a <tr> inside a <table>`);
			rowOnly(style, context.inRow);
			pageBreak(writer, style, "breakBefore", depth);
			const next = inherit(style, context, fonts);
			const paint = cellBox(boxStyle(style, next.size), context.cell);
			const streamed = streamMargin(paint, context, style);
			if (streamed && paint?.margin[0]) writer.spacer(paint.margin[0]);
			box(writer, streamed ? undefined : paint, () => {
				const row = style.display === "flex" && (style.flexDirection ?? "row") === "row";
				const inner = { ...next, inRow: false };
				const gap = style.gap === undefined ? 0 : point(style.gap, next.size);
				// A whitespace-only text child is no flex item, as in CSS.
				const items = node.children.filter((child) => child.kind !== "text" || !blank(child.value));
				if (row && items.length) {
					// A gap is a fixed-width empty column between items, so flex grow shares what is left.
					const spacer: Column = { kind: 0, value: gap };
					const columns = items.map(column);
					writer.rowStart(
						gap ? columns.flatMap((item, i) => (i ? [spacer, item] : [item])) : columns,
					);
					for (const [index, child] of items.entries()) {
						if (index && gap) writer.spacer(0);
						compile([child], writer, fonts, { ...next, inRow: true }, depth + 1, pageTokens);
					}
					writer.rowEnd();
				} else if (context.inRow || style.breakInside === "avoid") {
					writer.stackStart(gap);
					flow(node.children, writer, fonts, inner, depth + 1, pageTokens, 0);
					writer.stackEnd();
				} else flow(node.children, writer, fonts, inner, depth, pageTokens, gap);
			});
			if (streamed && paint?.margin[2]) writer.spacer(paint.margin[2]);
			pageBreak(writer, style, "breakAfter", depth);
			break;
		}
		case "a":
		case "span":
		case "b":
		case "strong":
		case "p":
		case "h1":
		case "h2":
		case "h3":
		case "h4":
		case "h5":
		case "h6": {
			rowOnly(style, context.inRow);
			pageBreak(writer, style, "breakBefore", depth);
			const next = inherit(style, context, fonts);
			const inner = { ...next, inRow: false, link: node.href ?? next.link };
			const runs = collect(node.children, inner, fonts);
			// A link around layout nodes (an image, a card) flows them as a block; every run inside links.
			if (runs === undefined && node.tag !== "a")
				throw new Error(
					`<${node.tag}> accepts only text, <br>, and <span>, <b>, <strong>, <a> without box styles`,
				);
			box(writer, boxStyle(style, next.size), () =>
				runs === undefined
					? flow(node.children, writer, fonts, inner, depth, pageTokens, 0)
					: writeParagraph(writer, runs, next, pageTokens),
			);
			pageBreak(writer, style, "breakAfter", depth);
			break;
		}
		case "img": {
			if (style.flex !== undefined && !context.inRow)
				throw new Error("flex applies only to a flex row child");
			if (typeof style.height === "string" && style.height.endsWith("%"))
				throw new Error("percent height has no container height; use pt or px, or set width");
			pageBreak(writer, style, "breakBefore", depth);
			const next = inherit(style, context, fonts);
			const width = style.width === undefined ? undefined : dimension(style.width, "width");
			const height =
				style.height === undefined ? undefined : dimension(style.height, "height").value;
			box(writer, boxStyle(style, next.size), () => {
				try {
					writer.image(node.src!, width, height, node.alt ?? "", context.link);
				} catch (error) {
					if (!(error instanceof Error)) throw error;
					if (error.message === "PageOverflow")
						throw new Error("PageOverflow: the image is taller than the page; reduce its height");
					if (error.message === "ImageTooWide")
						throw new Error("the image is wider than its container; reduce width or height");
					throw error;
				}
			});
			pageBreak(writer, style, "breakAfter", depth);
			break;
		}
		case "br":
			writeParagraph(writer, [lineBreak(node, context)], context, pageTokens);
			break;
		case "thead":
		case "tbody":
		case "tfoot":
		case "tr":
			throw new Error(`<${node.tag}> belongs in a <table>`);
		case "table":
			table(node, writer, fonts, context, depth, pageTokens);
			break;
		case "hr":
			rowOnly(style, context.inRow);
			pageBreak(writer, style, "breakBefore", depth);
			box(writer, boxStyle(style, inherit(style, context, fonts).size), () => {});
			pageBreak(writer, style, "breakAfter", depth);
			break;
	}
}

type RowGroup = { group?: StyledElement; rows: StyledElement[] };
/** A `td` or `th` and the grid area it covers; `row` counts from the table's first row. */
type GridCell = { node: StyledElement; row: number; column: number; columns: number; rows: number };
type GridRow = {
	node: StyledElement;
	state: Context;
	paint: Box | undefined;
	cells: GridCell[];
	slots: Slot[];
	/** A rowSpan from an earlier row reaches this one. */
	joined: boolean;
};
type GridGroup = { group?: StyledElement | undefined; rows: GridRow[] };
type Edge = { width: number; color: [number, number, number] };

const BLACK: [number, number, number] = [0, 0, 0];
const ALIGN = { top: 0, middle: 1, bottom: 2 } as const;
/** Rows and row groups sit inside the table, where `pageBreak` rejects both as nested. */
const BREAKS = ["breakBefore", "breakAfter"] as const;

/** `thead` groups first, then `tbody` and bare `tr` in source order, then `tfoot`, as browsers
 *  paint them. Adjacent bare rows form one group, as HTML's implied `tbody` does. */
function rowGroups(node: StyledElement): {
	header: RowGroup[];
	body: RowGroup[];
	footer: RowGroup[];
} {
	const header: RowGroup[] = [];
	const body: RowGroup[] = [];
	const footer: RowGroup[] = [];
	const accepts = "<table> accepts only <thead>, <tbody>, <tfoot>, and <tr>";
	for (const child of node.children) {
		if (child.kind === "text") throw new Error(`${accepts}, not text`);
		if (child.tag === "tr") {
			const last = body.at(-1);
			if (last && !last.group) last.rows.push(child);
			else body.push({ rows: [child] });
			continue;
		}
		if (child.tag !== "thead" && child.tag !== "tbody" && child.tag !== "tfoot")
			throw new Error(`${accepts}, not <${child.tag}>`);
		const rows = child.children.map((row) => {
			if (row.kind === "text" || row.tag !== "tr")
				throw locate(new Error(`<${child.tag}> accepts only <tr>`), child.where);
			return row;
		});
		(child.tag === "thead" ? header : child.tag === "tbody" ? body : footer).push({
			group: child,
			rows,
		});
	}
	return { header, body, footer };
}

function edge(box: Box | undefined, side: number): Edge {
	return box
		? { width: box.border[side]!, color: box.borderColor[side]! }
		: { width: 0, color: BLACK };
}

/** The wider border wins; on a tie the earlier candidate does. */
function widest(edges: Edge[]): Edge {
	return edges.reduce((best, next) => (next.width > best.width ? next : best));
}

/** CSS 2.1 collapsed borders: each edge takes the widest border of the cells and rows that
 *  meet there, a cell's beating its row's on a tie and the upper or left one beating the other.
 *  A cell draws half of each shared edge and all of an outer one, so each line draws once. */
function collapse(rows: GridRow[], width: number, boxes: ReadonlyMap<GridCell, Box | undefined>) {
	const at = rows.map((): GridCell[] => []);
	for (const row of rows)
		for (const cell of row.cells)
			for (let r = cell.row; r < cell.row + cell.rows; r++)
				for (let c = cell.column; c < cell.column + cell.columns; c++) at[r]![c] = cell;
	const near = (r: number, c: number) => boxes.get(at[r]![c]!);
	const tr = (r: number) => rows[r]!.paint;
	const paints = new Map<GridCell, CellPaint>();
	for (const row of rows)
		for (const cell of row.cells) {
			const own = boxes.get(cell);
			const { row: top, column: left } = cell;
			const bottom = top + cell.rows;
			const right = left + cell.columns;
			const across = Array.from({ length: cell.columns }, (_, index) => left + index);
			const down = Array.from({ length: cell.rows }, (_, index) => top + index);
			const shared = [top > 0, right < width, bottom < rows.length, left > 0];
			// Each side lists every segment's candidates in tie order.
			const sides = [
				across.map((c) =>
					shared[0]
						? [edge(near(top - 1, c), 2), edge(own, 0), edge(tr(top - 1), 2), edge(tr(top), 0)]
						: [edge(own, 0), edge(tr(top), 0)],
				),
				down.map((r) =>
					shared[1] ? [edge(own, 1), edge(near(r, right), 3)] : [edge(own, 1), edge(tr(r), 1)],
				),
				across.map((c) =>
					shared[2]
						? [edge(own, 2), edge(near(bottom, c), 0), edge(tr(bottom - 1), 2), edge(tr(bottom), 0)]
						: [edge(own, 2), edge(tr(bottom - 1), 2)],
				),
				down.map((r) =>
					shared[3] ? [edge(near(r, left - 1), 1), edge(own, 3)] : [edge(own, 3), edge(tr(r), 3)],
				),
			];
			const winners = sides.map((segments) => widest(segments.map(widest)));
			paints.set(cell, {
				border: winners.map((item, side) =>
					shared[side] ? item.width / 2 : item.width,
				) as Box["border"],
				borderColor: winners.map((item) => item.color) as Box["borderColor"],
			});
		}
	return paints;
}

function table(
	node: StyledElement,
	writer: ProtocolWriter,
	fonts: Fonts,
	context: Context,
	depth: number,
	pageTokens: boolean,
) {
	const style = node.style;
	if (style.flex !== undefined) throw new Error("flex applies only to a flex row child");
	pageBreak(writer, style, "breakBefore", depth);
	const next = inherit(style, context, fonts);
	const paint = boxStyle(style, next.size);
	if (
		paint &&
		(paint.margin[1] ||
			paint.margin[3] ||
			paint.background ||
			[...paint.padding, ...paint.border].some(Boolean))
	)
		throw new Error("<table> accepts only vertical margins; style its cells or a wrapping <div>");
	const collapsed = style.borderCollapse === "collapse";
	if (!collapsed && style.borderSpacing !== undefined && spacing(style.borderSpacing))
		throw new Error(
			"border-spacing is not implemented: cells tile their row; use cell padding, or border-collapse: collapse for single rules",
		);
	const inner = { ...next, inRow: false };
	const { header, body, footer } = rowGroups(node);

	// Places each cell in the next column that no rowSpan from an earlier row covers, as HTML does.
	let width = 0;
	const rows: GridRow[] = [];
	const place = ({ group, rows: trs }: RowGroup): GridGroup => {
		let state = inner;
		if (group)
			try {
				rowOnly(group.style, false);
				for (const key of BREAKS) pageBreak(writer, group.style, key, depth + 1);
				if (boxStyle(group.style, inner.size))
					throw new Error(`<${group.tag}> accepts no box styles; style <tr> or its cells`);
				state = { ...inherit(group.style, inner, fonts), inRow: false };
			} catch (error) {
				throw locate(error, group.where);
			}
		let covered: number[] = [];
		let reach = 0;
		const placed = trs.map((tr, index) => {
			try {
				rowOnly(tr.style, false);
				for (const key of BREAKS) pageBreak(writer, tr.style, key, depth + 1);
				const own = { ...inherit(tr.style, state, fonts), inRow: false };
				const row: GridRow = {
					node: tr,
					state: own,
					paint: boxStyle(tr.style, own.size),
					cells: [],
					slots: [],
					joined: index < reach,
				};
				const skip = () => {
					while (covered[row.slots.length]) row.slots.push("covered");
				};
				for (const cell of tr.children) {
					if (cell.kind === "text" || (cell.tag !== "td" && cell.tag !== "th"))
						throw new Error("<tr> accepts only <td> and <th>");
					skip();
					const column = row.slots.length;
					const columns = cell.colSpan ?? 1;
					const span = cell.rowSpan ?? 1;
					if (index + span > trs.length)
						throw locate(
							new Error(
								`rowSpan ${span} runs past the last row of its ${group ? `<${group.tag}>` : "row group"}; rows joined by rowSpan stay in one row group`,
							),
							cell.where,
						);
					if (columns > 1 && (cell.style.width !== undefined || cell.style.flex !== undefined))
						throw locate(
							new Error(
								"width and flex size one column; set them on a cell without colSpan in that column",
							),
							cell.where,
						);
					const align =
						ALIGN[
							cell.style.verticalAlign ??
								tr.style.verticalAlign ??
								group?.style.verticalAlign ??
								"top"
						];
					for (let c = column; c < column + columns; c++) {
						if (covered[c])
							throw locate(
								new Error("colSpan runs into a cell that rowSpan brings down from an earlier row"),
								cell.where,
							);
						covered[c] = span;
						row.slots.push(c === column ? { align, rows: span } : "join");
					}
					row.cells.push({ node: cell, row: rows.length, column, columns, rows: span });
					reach = Math.max(reach, index + span);
				}
				skip();
				if (!row.slots.length) throw new Error("<tr> has no cells");
				width ||= row.slots.length;
				if (row.slots.length !== width)
					throw new Error(`<tr> has ${row.slots.length} cells but the table has ${width} columns`);
				covered = covered.map((count) => count - 1);
				rows.push(row);
				return row;
			} catch (error) {
				throw locate(error, tr.where);
			}
		});
		return { group, rows: placed };
	};
	const [heads, bodies, feet] = [header, body, footer].map((groups) => groups.map(place));
	if (!rows.length) throw new Error("<table> has no rows");
	const cells = rows.flatMap((row) => row.cells);
	// Each column takes its width from its first cell without colSpan.
	const columns = Array.from({ length: width }, (_, index): Column => {
		const cell = cells.find((item) => item.column === index && item.columns === 1);
		return cell ? column(cell.node) : { kind: 1, value: 1 };
	});
	const paints = collapsed
		? collapse(
				rows,
				width,
				new Map(
					rows.flatMap((row) =>
						row.cells.map((cell) => {
							try {
								const size = inherit(cell.node.style, row.state, fonts).size;
								return [cell, boxStyle(cell.node.style, size)] as const;
							} catch (error) {
								throw locate(error, cell.node.where);
							}
						}),
					),
				),
			)
		: undefined;
	// A parent flex row already consumed `width` as this table's track.
	const track: Column = context.inRow ? { kind: 1, value: 1 } : column(node);
	if (paint?.margin[0]) writer.spacer(paint.margin[0]);
	/** Element path of each table child, indexed as the renderer reports an overflowing row. */
	const wheres: { where: string; joined: boolean }[] = [];
	const row = (item: GridRow, joined: boolean) => {
		try {
			// Collapsed borders belong to the cells.
			const own =
				collapsed && item.paint
					? { ...item.paint, border: [0, 0, 0, 0] as Box["border"] }
					: item.paint;
			const shaped = own && [...own.margin, ...own.padding, ...own.border].some(Boolean);
			if (joined && shaped)
				throw new Error(
					"a <tr> that rowSpan joins to other rows takes only a background; style its cells",
				);
			// Joined rows lay out as one block, so each row's background paints in its own cells.
			box(writer, joined || !(shaped || own?.background) ? undefined : own, () => {
				writer.rowStart(columns, item.slots);
				for (const cell of item.cells)
					compile(
						[cell.node],
						writer,
						fonts,
						{
							...item.state,
							inRow: true,
							cell: { ...paints?.get(cell), ...(joined && { background: own?.background }) },
						},
						depth + 2,
						pageTokens,
					);
				writer.rowEnd();
			});
		} catch (error) {
			throw locate(error, item.node.where);
		}
	};
	const group = ({ group, rows }: GridGroup, repeated: boolean) => {
		// Header and footer rows always share a page, so `break-inside: avoid` only groups body rows.
		const avoid = group?.style.breakInside === "avoid" && !repeated;
		// Each path goes in first: the engine rejects an overflowing child the moment it closes.
		if (avoid) {
			wheres.push({ where: group!.where, joined: false });
			writer.stackStart(0);
		}
		for (let start = 0; start < rows.length;) {
			let end = start + 1;
			while (rows[end]?.joined) end++;
			// Rows that rowSpan joins move between pages together, like break-inside: avoid.
			const joined = end - start > 1;
			if (!avoid) wheres.push({ where: rows[start]!.node.where, joined });
			if (joined) writer.stackStart(0);
			for (const item of rows.slice(start, end)) row(item, joined);
			if (joined) writer.stackEnd();
			start = end;
		}
		if (avoid) writer.stackEnd();
	};
	try {
		if (style.breakInside === "avoid") writer.stackStart(0);
		const count = (groups: GridGroup[]) =>
			groups.reduce((total, { rows }) => total + rows.filter((item) => !item.joined).length, 0);
		writer.tableStart(count(heads!), count(feet!), track);
		// Footer rows precede the body so the engine can reserve them and place each body row on arrival.
		for (const item of heads!) group(item, true);
		for (const item of feet!) group(item, true);
		for (const item of bodies!) group(item, false);
		writer.tableEnd();
		if (style.breakInside === "avoid") writer.stackEnd();
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		// A row that overflows as it closes inside `row()` already carries its path.
		const overflow = /^TableRowOverflow\((\d+)\)/.exec(error.message);
		if (overflow) {
			const child = wheres[Number(overflow[1])];
			throw locate(
				new Error(
					child?.joined
						? "rows that rowSpan joins are taller than the page beside the repeated header and footer; shorten the spanning cell or split the rows"
						: "a table row is taller than the page beside its repeated header and footer; split its content",
				),
				child?.where ?? node.where,
			);
		}
		if (error.message === "PageOverflow")
			throw locate(
				new Error(
					style.breakInside === "avoid"
						? "PageOverflow: the table is taller than a page; drop break-inside: avoid"
						: "PageOverflow: the table header and footer are taller than a page",
				),
				node.where,
			);
		throw error;
	}
	if (paint?.margin[2]) writer.spacer(paint.margin[2]);
	pageBreak(writer, style, "breakAfter", depth);
}

export function compile(
	nodes: readonly StyledNode[],
	writer: ProtocolWriter,
	fonts: Fonts,
	context: Context = ROOT,
	depth = 0,
	pageTokens = false,
): void {
	for (const node of nodes) {
		if (node.kind === "text") {
			writeParagraph(writer, [run(context, node.value)], context, pageTokens);
			continue;
		}
		try {
			element(node, writer, fonts, context, depth, pageTokens);
		} catch (error) {
			throw locate(engineError(glyphError(error)), node.where);
		}
	}
}
