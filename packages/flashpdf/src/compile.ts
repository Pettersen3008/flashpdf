import type { StyledNode } from "./css.js";
import type { StyledElement } from "./css/cascade.js";
import { PAGE_NUMBER, TOTAL_PAGES } from "./page.js";
import type { ProtocolWriter, Column, Run } from "./protocol.js";
import {
	boxStyle,
	color,
	font,
	helvetica,
	point,
	type Box,
	type FontSlots,
	type NormalizedStyle,
} from "./style.js";
import { locate, rowOnly } from "./tree.js";

type TextState = {
	size: number;
	align: "left" | "center" | "right";
	color: [number, number, number];
	family: FontSlots;
	bold: boolean;
};
type Context = TextState & { inRow: boolean };
type Fonts = ReadonlyMap<string, FontSlots>;

const ROOT: Context = {
	size: 12,
	align: "left",
	color: [0, 0, 0],
	family: helvetica,
	bold: false,
	inRow: false,
};

const INLINE_TAGS = new Set<string>(["span", "b", "strong", "br"]);

function inherit(style: NormalizedStyle, current: TextState, fonts: Fonts): TextState {
	const selected = font(style, current.family, current.bold, fonts);
	return {
		size: style.fontSize === undefined ? current.size : point(style.fontSize, current.size, true),
		align: style.textAlign ?? current.align,
		color: style.color === undefined ? current.color : color(style.color),
		family: selected.family,
		bold: selected.weight,
	};
}

function run(state: TextState, text: string, hardBreak = false): Run {
	return {
		font: state.bold ? state.family.bold! : state.family.regular,
		size: state.size,
		color: state.color,
		text,
		break: hardBreak,
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
	if (node.tag === "br") return [run(context, "", true)];
	try {
		const style = node.style;
		rowOnly(style, context.inRow);
		if (
			boxStyle(style, context.size) ||
			style.breakBefore === "page" ||
			style.breakAfter === "page"
		)
			return undefined;
		const next = inherit(style, context, fonts);
		if (next.align !== context.align) return undefined;
		return collect(node.children, { ...next, inRow: false }, fonts);
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
		throw new Error("page tokens are only valid in a footer");
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
		if (!runs.length) return;
		separate();
		writeParagraph(writer, runs, context, pageTokens);
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

function column(value: StyledNode): Column {
	if (value.kind === "text") return { kind: 1, value: 1 };
	const { flex, width } = value.style;
	if (typeof flex === "number" && flex > 0) return { kind: 1, value: flex };
	if (typeof width === "number") return { kind: 0, value: width };
	if (typeof width === "string") {
		const amount = Number.parseFloat(width);
		return width.endsWith("pt")
			? { kind: 0, value: amount }
			: width.endsWith("px")
				? { kind: 0, value: amount * 0.75 }
				: { kind: 2, value: amount };
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
	switch (node.tag) {
		case "div":
		case "main":
		case "section":
		case "article":
		case "header":
		case "footer":
		case "td":
		case "th": {
			rowOnly(style, context.inRow);
			pageBreak(writer, style, "breakBefore", depth);
			const next = inherit(style, context, fonts);
			const paint = boxStyle(style, next.size);
			const streamed = streamMargin(paint, context, style);
			if (streamed && paint?.margin[0]) writer.spacer(paint.margin[0]);
			box(writer, streamed ? undefined : paint, () => {
				const row = style.display === "flex" && (style.flexDirection ?? "row") === "row";
				const inner = { ...next, inRow: false };
				const gap = style.gap === undefined ? 0 : point(style.gap, next.size);
				if (row && node.children.length) {
					if (gap !== 0) throw new Error("row gap is not implemented");
					writer.rowStart(node.children.map(column));
					compile(node.children, writer, fonts, { ...next, inRow: true }, depth + 1, pageTokens);
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
			const runs = collect(node.children, { ...next, inRow: false }, fonts);
			if (runs === undefined)
				throw new Error(
					`<${node.tag}> accepts only text, <br>, and <span>, <b>, <strong> without box styles`,
				);
			box(writer, boxStyle(style, next.size), () => writeParagraph(writer, runs, next, pageTokens));
			pageBreak(writer, style, "breakAfter", depth);
			break;
		}
		case "br":
			writeParagraph(writer, [run(context, "", true)], context, pageTokens);
			break;
		case "thead":
		case "tbody":
		case "tfoot":
		case "tr":
			throw new Error(`<${node.tag}> belongs in a <table>`);
		case "table":
			table(node, writer, fonts, context, depth, pageTokens);
			break;
		case "hr": {
			const next = inherit(style, context, fonts);
			const rule =
				style.borderWidth === undefined &&
				style.borderBottomWidth === undefined &&
				style.borderBottom === undefined;
			box(
				writer,
				boxStyle(rule ? { ...style, borderBottom: "1pt solid black" } : style, next.size),
				() => {},
			);
			break;
		}
	}
}

type RowGroup = { group?: StyledElement; rows: StyledElement[] };
/** Rows and row groups sit inside the table, where `pageBreak` rejects both as nested. */
const BREAKS = ["breakBefore", "breakAfter"] as const;

/** `thead` groups first, then `tbody` and bare `tr` in source order, then `tfoot`, as browsers paint them. */
function rowGroups(node: StyledElement): { header: RowGroup[]; body: RowGroup[] } {
	const header: RowGroup[] = [];
	const body: RowGroup[] = [];
	const footer: RowGroup[] = [];
	const accepts = "<table> accepts only <thead>, <tbody>, <tfoot>, and <tr>";
	for (const child of node.children) {
		if (child.kind === "text") throw new Error(`${accepts}, not text`);
		if (child.tag === "tr") {
			body.push({ rows: [child] });
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
	return { header, body: [...body, ...footer] };
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
	const { header, body } = rowGroups(node);
	const rows = [...header, ...body].flatMap((group) => group.rows);
	const first = rows[0];
	if (!first) throw new Error("<table> has no rows");
	const cells = (row: StyledElement) =>
		row.children.map((cell) => {
			if (cell.kind === "text" || (cell.tag !== "td" && cell.tag !== "th"))
				throw locate(new Error("<tr> accepts only <td> and <th>"), row.where);
			return cell;
		});
	const columns = cells(first).map(column);
	for (const row of rows) {
		const count = cells(row).length;
		if (count !== columns.length)
			throw locate(
				new Error(`<tr> has ${count} cells but the table has ${columns.length} columns`),
				row.where,
			);
	}
	// A parent flex row already consumed `width` as this table's track.
	const width: Column = context.inRow ? { kind: 1, value: 1 } : column(node);
	if (paint?.margin[0]) writer.spacer(paint.margin[0]);
	const inner = { ...next, inRow: false };
	/** Element path of each table child, indexed as the renderer reports an overflowing row. */
	const wheres: string[] = [];
	const row = (tr: StyledElement, state: Context) => {
		try {
			rowOnly(tr.style, false);
			for (const key of BREAKS) pageBreak(writer, tr.style, key, depth + 1);
			const own = inherit(tr.style, state, fonts);
			box(writer, boxStyle(tr.style, own.size), () => {
				writer.rowStart(columns);
				compile(tr.children, writer, fonts, { ...own, inRow: true }, depth + 2, pageTokens);
				writer.rowEnd();
			});
		} catch (error) {
			throw locate(error, tr.where);
		}
	};
	const group = ({ group, rows }: RowGroup, repeated: boolean) => {
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
		// Header rows always share a page, so `break-inside: avoid` only groups body rows.
		if (group?.style.breakInside === "avoid" && !repeated) {
			writer.stackStart(0);
			for (const tr of rows) row(tr, state);
			writer.stackEnd();
			wheres.push(group.where);
			return;
		}
		for (const tr of rows) {
			row(tr, state);
			wheres.push(tr.where);
		}
	};
	try {
		if (style.breakInside === "avoid") writer.stackStart(0);
		writer.tableStart(
			header.reduce((count, { rows }) => count + rows.length, 0),
			width,
		);
		for (const item of header) group(item, true);
		for (const item of body) group(item, false);
		writer.tableEnd();
		if (style.breakInside === "avoid") writer.stackEnd();
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		const overflow = /^TableRowOverflow\((\d+)\)$/.exec(error.message);
		if (overflow)
			throw locate(
				new Error(
					"a table row is taller than the page below its repeated header; split its content",
				),
				wheres[Number(overflow[1])] ?? node.where,
			);
		if (error.message === "PageOverflow")
			throw locate(
				new Error(
					style.breakInside === "avoid"
						? "PageOverflow: the table is taller than a page; drop break-inside: avoid"
						: "PageOverflow: the table header is taller than a page",
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
			throw locate(error, node.where);
		}
	}
}
