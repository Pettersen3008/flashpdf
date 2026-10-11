import type { StyledNode } from "./css.js";
import type { StyledElement } from "./css/cascade.js";
import { inherited } from "./css/properties.js";
import { glyphError } from "./fonts.js";
import { PAGE_NUMBER, TOTAL_PAGES } from "./page.js";
import type { ProtocolWriter, Column, FlexItem, Run } from "./protocol.js";
import {
	boxSize,
	boxStyle,
	color,
	decoration,
	dimension,
	flexBasis,
	font,
	helvetica,
	lineHeight,
	point,
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
/** `inRow` marks one row or flex item, `cell` a table cell, and `item` a CSS flex item, whose
 *  width is its own. `height` is the parent's fixed content height, for percent heights. */
type Context = TextState & {
	inRow: boolean;
	cell?: boolean;
	item?: boolean;
	height?: number | undefined;
};
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
/** Tags whose `display: flex` lays out their children. */
const BLOCK_TAGS = new Set<string>([
	"div",
	"main",
	"section",
	"article",
	"header",
	"footer",
	"td",
	"th",
]);
const SIZES = ["width", "minWidth", "maxWidth", "height", "minHeight", "maxHeight"] as const;
const MARGINS = ["marginTop", "marginRight", "marginBottom", "marginLeft"] as const;
const EMPTY: Box = {
	margin: [0, 0, 0, 0],
	padding: [0, 0, 0, 0],
	border: [0, 0, 0, 0],
	background: undefined,
	borderColor: [
		[0, 0, 0],
		[0, 0, 0],
		[0, 0, 0],
		[0, 0, 0],
	],
};
const JUSTIFY: Record<string, number> = {
	normal: 0,
	"flex-start": 0,
	start: 0,
	"flex-end": 1,
	end: 1,
	center: 2,
	"space-between": 3,
	"space-around": 4,
	"space-evenly": 5,
};
const ALIGN: Record<string, number> = {
	auto: 0,
	normal: 0,
	stretch: 1,
	"flex-start": 2,
	start: 2,
	"flex-end": 3,
	end: 3,
	center: 4,
};

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
		"TextTooWide: a character, or a header or footer line, is wider than its space; reduce the font size or widen the column",
	],
	[
		"ContentOverflow",
		"ContentOverflow: the content is taller than its box's height or max-height, or than its flex container; raise the height or shorten the content",
	],
]);
/** Explains the engine's bare error codes; `locate` then names the element. */
function engineError(error: unknown): unknown {
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
		if (
			boxStyle(style, context.size) ||
			sized(style) ||
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
			throw new Error(
				(value.size?.height ?? value.size?.maxHeight) === undefined
					? "PageOverflow: use document margins or split this decorated box"
					: "PageOverflow: a box with a height or max-height moves whole, and this one is taller than a page",
			);
		throw error;
	}
}

/** Sizes or an auto margin make a layout node; `box-sizing` alone changes nothing. */
function sized(s: NormalizedStyle) {
	return SIZES.some((key) => s[key] !== undefined) || MARGINS.some((key) => s[key] === "auto");
}

/** Rejects the sizes and auto margins an image or a table part cannot honour. */
function sizeless(s: NormalizedStyle, tag: string, keep: readonly string[] = []) {
	for (const key of [...SIZES, "boxSizing"] as const)
		if (s[key] !== undefined && !keep.includes(key))
			throw new Error(`${key} applies to blocks and flex items, not <${tag}>`);
	if (MARGINS.some((key) => s[key] === "auto"))
		throw new Error(`auto margins apply to blocks and flex items, not <${tag}>`);
}

/** The box an element paints and sizes. A column-model row or a table owns its items' width. */
function layoutBox(style: NormalizedStyle, size: number, context: Context): Box | undefined {
	const paint = boxStyle(style, size);
	const sizes = boxSize(style, context.height, !context.inRow || context.item === true);
	return sizes ? { ...(paint ?? EMPTY), size: sizes } : paint;
}

/** Bounds of a box's content height; equal when `height` fixes it. */
function heights(box: Box | undefined): [number, number | undefined] {
	const size = box?.size;
	if (!size) return [0, undefined];
	const frame = box.padding[0] + box.padding[2] + box.border[0] + box.border[2];
	const inner = (value: number) => (size.borderBox ? Math.max(0, value - frame) : value);
	const min = inner(size.minHeight ?? 0);
	const max = size.maxHeight === undefined ? undefined : inner(size.maxHeight);
	if (size.height === undefined) return [min, max];
	const fixed = Math.max(Math.min(inner(size.height), max ?? Infinity), min);
	return [fixed, fixed];
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
		!box.size &&
		box.margin[1] === 0 &&
		box.margin[3] === 0 &&
		box.padding.every((edge) => edge === 0) &&
		box.border.every((edge) => edge === 0) &&
		box.background === undefined &&
		!context.inRow &&
		style.breakInside !== "avoid",
	);
}

/** A column-model track: a width, or a grow factor with a zero basis, or with `auto` and no
 *  width. Undefined for any other flex, which needs the CSS flex algorithm. */
function track(s: NormalizedStyle): Column | undefined {
	const grow = s.flexGrow ?? 0;
	const basis = s.flexBasis === undefined ? undefined : flexBasis(s.flexBasis);
	if (basis && basis.value > 0) return undefined;
	if (grow > 0 && (basis || s.width === undefined)) return { kind: 1, value: grow };
	if (s.width !== undefined && grow === 0) {
		const { kind, value } = dimension(s.width, "width");
		return { kind: kind === 2 ? 2 : 0, value };
	}
	const flexed =
		s.flexGrow !== undefined || s.flexShrink !== undefined || s.flexBasis !== undefined;
	return flexed ? undefined : { kind: 1, value: 1 };
}

function column(value: StyledNode): Column {
	if (value.kind === "text") return { kind: 1, value: 1 };
	const result = track(value.style);
	if (!result)
		throw locate(
			new Error(
				"a table column takes a width or a flex grow factor, not a flex basis or flex: none",
			),
			value.where,
		);
	return result;
}

/** Earlier releases measured flex rows as columns: an item without flex or width took an equal
 *  share. A row keeps that model, so its output is unchanged, until it or an item uses a
 *  property the model cannot express. A column without one stays block flow. */
function cssFlex(style: NormalizedStyle, items: readonly StyledNode[], column: boolean) {
	const own = [
		style.justifyContent,
		style.alignItems,
		style.height,
		style.minHeight,
		style.maxHeight,
	];
	if (own.some((value) => value !== undefined)) return true;
	return items.some((item) => {
		if (item.kind === "text") return false;
		const s = item.style;
		if (s.alignSelf !== undefined || MARGINS.some((key) => s[key] === "auto")) return true;
		if (column)
			return s.flexGrow !== undefined || s.flexShrink !== undefined || s.flexBasis !== undefined;
		const sizes = [s.minWidth, s.maxWidth, s.height, s.minHeight, s.maxHeight, s.boxSizing];
		return sizes.some((value) => value !== undefined) || track(s) === undefined;
	});
}

/** A CSS flex container: one record for the container and its items, then one element per item. */
function flexbox(
	node: StyledElement,
	items: readonly StyledNode[],
	writer: ProtocolWriter,
	fonts: Fonts,
	text: TextState,
	layout: Box | undefined,
	depth: number,
	pageTokens: boolean,
) {
	const style = node.style;
	const column = style.flexDirection === "column";
	const [minHeight, maxHeight] = heights(layout);
	const justify = JUSTIFY[style.justifyContent ?? "normal"]!;
	const specs = items.map((item): FlexItem => {
		if (item.kind === "text") return { grow: 0, shrink: 1, basis: undefined, align: 0 };
		const s = item.style;
		return {
			grow: s.flexGrow ?? 0,
			shrink: s.flexShrink ?? 1,
			basis: s.flexBasis === undefined ? undefined : flexBasis(s.flexBasis),
			align: ALIGN[s.alignSelf ?? "auto"]!,
		};
	});
	if (column) {
		const flexed = items.findIndex((_, index) => specs[index]!.grow > 0 || specs[index]!.basis);
		if (flexed >= 0)
			throw locate(
				new Error("flex-grow and flex-basis size flex row items; give a column item a height"),
				(items[flexed] as StyledElement).where,
			);
		// Without a height the column is as tall as its items, so there is no space to distribute.
		if (justify && layout?.size?.height === undefined && !layout?.size?.minHeight)
			throw new Error(
				"justify-content in a flex column needs a height or min-height on the column",
			);
	}
	writer.flexStart(
		{
			column,
			justify,
			align: ALIGN[style.alignItems ?? "normal"]!,
			gap: style.gap === undefined ? 0 : point(style.gap, text.size),
			minHeight,
			maxHeight,
		},
		specs,
	);
	const inner: Context = {
		...text,
		inRow: true,
		item: true,
		height: minHeight === maxHeight ? minHeight : undefined,
	};
	for (const child of items) compile([child], writer, fonts, inner, depth + 1, pageTokens);
	writer.flexEnd();
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
	if (
		(style.justifyContent ?? style.alignItems) !== undefined &&
		(style.display !== "flex" || !BLOCK_TAGS.has(node.tag))
	)
		throw new Error("justify-content and align-items apply only to a flex container");
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
			if (context.cell) sizeless(style, node.tag, ["width"]);
			rowOnly(style, context.inRow);
			pageBreak(writer, style, "breakBefore", depth);
			const next = inherit(style, context, fonts);
			const paint = layoutBox(style, next.size, context);
			const streamed = streamMargin(paint, context, style);
			if (streamed && paint?.margin[0]) writer.spacer(paint.margin[0]);
			// A CSS flex item is one element, so one without a box of its own gets an empty one.
			box(writer, streamed ? undefined : (paint ?? (context.item ? EMPTY : undefined)), () => {
				const flex = style.display === "flex";
				const row = flex && (style.flexDirection ?? "row") === "row";
				const [minHeight, maxHeight] = heights(paint);
				const inner = {
					...next,
					inRow: false,
					height: minHeight === maxHeight ? minHeight : undefined,
				};
				const gap = style.gap === undefined ? 0 : point(style.gap, next.size);
				// A whitespace-only text child is no flex item, as in CSS.
				const items = node.children.filter((child) => child.kind !== "text" || !blank(child.value));
				if (flex && items.length && cssFlex(style, items, !row))
					flexbox(node, items, writer, fonts, next, paint, depth, pageTokens);
				else if (row && items.length) {
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
				} else if ((context.inRow && !context.item) || style.breakInside === "avoid") {
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
			const own = layoutBox(style, next.size, context);
			box(writer, own ?? (context.item && runs === undefined ? EMPTY : undefined), () =>
				runs === undefined
					? flow(node.children, writer, fonts, inner, depth, pageTokens, 0)
					: writeParagraph(writer, runs, next, pageTokens),
			);
			pageBreak(writer, style, "breakAfter", depth);
			break;
		}
		case "img": {
			rowOnly(style, context.inRow);
			sizeless(style, "img", ["width", "height"]);
			if (typeof style.height === "string" && style.height.endsWith("%"))
				throw new Error("percent height has no container height; use pt or px, or set width");
			pageBreak(writer, style, "breakBefore", depth);
			const next = inherit(style, context, fonts);
			const width = style.width === undefined ? undefined : dimension(style.width, "width");
			if (context.item && width?.kind === 2)
				throw new Error(
					"an image flex item takes a width in pt or px; wrap it to size it by percent",
				);
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
			box(writer, layoutBox(style, inherit(style, context, fonts).size, context), () => {});
			pageBreak(writer, style, "breakAfter", depth);
			break;
	}
}

type RowGroup = { group?: StyledElement; rows: StyledElement[] };
/** Rows and row groups sit inside the table, where `pageBreak` rejects both as nested. */
const BREAKS = ["breakBefore", "breakAfter"] as const;

/** `thead` groups first, then `tbody` and bare `tr` in source order, then `tfoot`, as browsers paint them. */
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
	return { header, body, footer };
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
	if ((style.flexGrow ?? style.flexShrink ?? style.flexBasis) !== undefined)
		throw new Error("flex applies only to a flex row child");
	sizeless(style, "table", ["width"]);
	if (context.item && style.width !== undefined)
		throw new Error("a table in a flex container takes no width; set it on a wrapping <div>");
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
	const { header, body, footer } = rowGroups(node);
	const rows = [...header, ...body, ...footer].flatMap((group) => group.rows);
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
			sizeless(tr.style, "tr");
			for (const key of BREAKS) pageBreak(writer, tr.style, key, depth + 1);
			const own = inherit(tr.style, state, fonts);
			box(writer, boxStyle(tr.style, own.size), () => {
				writer.rowStart(columns);
				compile(
					tr.children,
					writer,
					fonts,
					{ ...own, inRow: true, cell: true },
					depth + 2,
					pageTokens,
				);
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
				sizeless(group.style, group.tag);
				for (const key of BREAKS) pageBreak(writer, group.style, key, depth + 1);
				if (boxStyle(group.style, inner.size))
					throw new Error(`<${group.tag}> accepts no box styles; style <tr> or its cells`);
				state = { ...inherit(group.style, inner, fonts), inRow: false };
			} catch (error) {
				throw locate(error, group.where);
			}
		// Header and footer rows always share a page, so `break-inside: avoid` only groups body rows.
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
		const count = (groups: RowGroup[]) =>
			groups.reduce((total, { rows }) => total + rows.length, 0);
		writer.tableStart(count(header), count(footer), width);
		for (const item of header) group(item, true);
		for (const item of body) group(item, false);
		for (const item of footer) group(item, true);
		writer.tableEnd();
		if (style.breakInside === "avoid") writer.stackEnd();
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		const overflow = /^TableRowOverflow\((\d+)\)$/.exec(error.message);
		if (overflow)
			throw locate(
				new Error(
					"a table row is taller than the page beside its repeated header and footer; split its content",
				),
				wheres[Number(overflow[1])] ?? node.where,
			);
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
