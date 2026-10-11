import type { StyledNode } from "./css.js";
import type { StyledElement } from "./css/cascade.js";
import { inherited } from "./css/properties.js";
import { glyphError } from "./fonts.js";
import { PAGE_NUMBER, TOTAL_PAGES } from "./page.js";
import type { ProtocolWriter, Column, Run } from "./protocol.js";
import {
	boxStyle,
	color,
	counter,
	decoration,
	decorationColor,
	dimension,
	face,
	font,
	helvetica,
	kerning,
	lineHeight,
	listStylePosition,
	listStyleType,
	point,
	SHAPES,
	spacing,
	transform,
	verticalAlign,
	whiteSpace,
	type Box,
	type FontSlots,
	type LineHeight,
	type NormalizedStyle,
	type Transform,
	type WhiteSpace,
} from "./style.js";
import { locate, rowOnly } from "./tree.js";

type TextState = {
	size: number;
	align: "left" | "center" | "right" | "justify";
	color: [number, number, number];
	family: FontSlots;
	bold: boolean;
	italic: boolean;
	kerning: boolean;
	underline: boolean;
	lineThrough: boolean;
	/** Undefined is the text colour. */
	decorationColor: [number, number, number] | undefined;
	lineHeight: LineHeight;
	letterSpacing: number;
	wordSpacing: number;
	transform: Transform;
	whiteSpace: WhiteSpace;
	/** Baseline shift in points, up positive; `sub` and `sup` add to their parent's. */
	rise: number;
	/** The background of the nearest enclosing `<mark>`. */
	background: [number, number, number] | undefined;
	/** The nearest enclosing `<a href>`. */
	link: string | undefined;
	/** Set on the direct children of a `ul` or `ol`, whose items number themselves from it. */
	list: { next: number; step: number; ordered: boolean } | undefined;
	/** The list item being compiled, which its first paragraph empties by taking the marker. */
	item: { marker: Run | undefined } | undefined;
};
/** `cell` is set only for the direct children of a table row, `inColumn` for those of a flex column. */
type Context = TextState & { inRow: boolean; cell?: boolean; inColumn?: boolean };
type Fonts = ReadonlyMap<string, FontSlots>;

const ROOT: Context = {
	size: 12,
	align: "left",
	color: [0, 0, 0],
	family: helvetica,
	bold: false,
	italic: false,
	kerning: true,
	underline: false,
	lineThrough: false,
	decorationColor: undefined,
	lineHeight: undefined,
	letterSpacing: 0,
	wordSpacing: 0,
	transform: "none",
	whiteSpace: "normal",
	rise: 0,
	background: undefined,
	link: undefined,
	list: undefined,
	item: undefined,
	inRow: false,
};

const INLINE_TAGS = new Set<string>([
	"span",
	"b",
	"strong",
	"em",
	"i",
	"cite",
	"a",
	"br",
	"small",
	"sub",
	"sup",
	"u",
	"ins",
	"s",
	"del",
	"mark",
	"code",
	"kbd",
	"samp",
]);
const INLINE_ONLY =
	"accepts only text, <br>, and inline elements such as <span>, <b>, <a>, and <code> without box styles";
const NBSP = "\u00a0";
/** Blocks with no line of text of their own, so a marker before them takes a line of its own. */
const MARKER_ALONE = new Set<string>(["ul", "ol", "table", "img", "hr"]);

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
		"LineTooWide",
		"LineTooWide: a line of white-space: pre or nowrap text is wider than its container; shorten it, reduce the font size, or use white-space: pre-wrap",
	],
]);
/** Explains the engine's bare error codes; `locate` then names the element. */
function engineError(error: unknown): unknown {
	const message = error instanceof Error ? ENGINE_ERRORS.get(error.message) : undefined;
	return message ? new Error(message, { cause: error }) : error;
}

function inherit(style: NormalizedStyle, current: TextState, fonts: Fonts): TextState {
	const selected = font(style, current.family, current.bold, current.italic, fonts);
	const decorated = style.textDecoration === undefined ? current : decoration(style.textDecoration);
	const size =
		style.fontSize === undefined ? current.size : point(style.fontSize, current.size, true);
	return {
		size,
		align: style.textAlign ?? current.align,
		color: style.color === undefined ? current.color : color(style.color),
		family: selected.family,
		bold: selected.weight,
		italic: selected.italic,
		kerning: style.fontKerning === undefined ? current.kerning : kerning(style.fontKerning),
		underline: decorated.underline,
		lineThrough: decorated.lineThrough,
		decorationColor:
			style.textDecorationColor === undefined
				? current.decorationColor
				: decorationColor(style.textDecorationColor),
		lineHeight:
			style.lineHeight === undefined ? current.lineHeight : lineHeight(style.lineHeight, size),
		letterSpacing:
			style.letterSpacing === undefined
				? current.letterSpacing
				: spacing(style.letterSpacing, size, "letterSpacing"),
		wordSpacing:
			style.wordSpacing === undefined
				? current.wordSpacing
				: spacing(style.wordSpacing, size, "wordSpacing"),
		transform:
			style.textTransform === undefined ? current.transform : transform(style.textTransform),
		whiteSpace: style.whiteSpace === undefined ? current.whiteSpace : whiteSpace(style.whiteSpace),
		rise:
			current.rise +
			(style.verticalAlign === undefined ? 0 : verticalAlign(style.verticalAlign, current.size)),
		background: current.background,
		link: current.link,
		list: undefined,
		item: current.item,
	};
}

function run(state: TextState, text: string, hardBreak = false): Run {
	const height = state.lineHeight;
	const decorated = state.underline || state.lineThrough;
	return {
		...face(state.family, state.bold, state.italic),
		size: state.size,
		lineHeight: height === undefined ? 0 : "pt" in height ? height.pt : height.factor * state.size,
		color: state.color,
		text,
		break: hardBreak,
		underline: state.underline,
		lineThrough: state.lineThrough,
		decorationColor: decorated ? state.decorationColor : undefined,
		link: state.link,
		letterSpacing: state.letterSpacing,
		wordSpacing: state.wordSpacing,
		kerning: state.kerning,
		rise: state.rise,
		background: state.background,
		marker: 0,
		whiteSpace: state.whiteSpace,
		transform: state.transform,
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
		String(last.decorationColor) === String(next.decorationColor) &&
		last.link === next.link &&
		last.oblique === next.oblique &&
		last.letterSpacing === next.letterSpacing &&
		last.wordSpacing === next.wordSpacing &&
		last.kerning === next.kerning &&
		last.rise === next.rise &&
		String(last.background) === String(next.background) &&
		last.whiteSpace === next.whiteSpace &&
		last.transform === next.transform &&
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
		// A `<mark>` paints its background behind each line of its text, so only box edges make it a box.
		const paint = boxStyle(style, context.size);
		const highlight =
			node.tag === "mark" &&
			paint?.margin.concat(paint.padding, paint.border).every((edge) => !edge);
		if ((paint && !highlight) || style.breakBefore === "page" || style.breakAfter === "page")
			return undefined;
		const next = inherit(style, context, fonts);
		if (highlight && paint?.background) next.background = paint.background;
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
	// A word may span runs, so each run sees the text before it.
	let before = "";
	const shown = preserve(
		runs.map((item) => {
			const text = transformText(item.text, item.transform, before);
			before = item.break ? "" : item.text;
			return { ...item, text };
		}),
	);
	// The first paragraph of a list item takes its marker, which hangs before that paragraph's first line.
	if (state.item?.marker) {
		shown.unshift(state.item.marker);
		state.item.marker = undefined;
	}
	writer.paragraph(shown, state.align);
}

/** Applies `white-space` to a paragraph's runs. In `pre`, `pre-wrap`, and `pre-line` a newline ends
 *  the line. The engine collapses every ASCII space, so `pre` and `pre-wrap` keep spaces as U+00A0,
 *  but `pre-wrap` leaves the last of a run of spaces as a space, where the line may wrap. `nowrap`
 *  collapses to one U+00A0 so its words never break. A tab reaches the next multiple of 8 columns. */
function preserve(runs: readonly Run[]): Run[] {
	const result: Run[] = [];
	let column = 0;
	// The line has text, and it ends in a space that may wrap, or in a collapsed one.
	let started = false;
	let loose = false;
	for (const [index, item] of runs.entries()) {
		const mode = item.whiteSpace;
		if (mode === "normal") {
			result.push(item);
			continue;
		}
		let piece = "";
		let spaces = 0;
		const flush = () => {
			if (!spaces) return;
			const kept = mode === "pre-wrap" && started && !loose ? spaces - 1 : spaces;
			piece += NBSP.repeat(kept) + " ".repeat(spaces - kept);
			loose = kept < spaces;
			column += spaces;
			spaces = 0;
		};
		for (const char of item.text.replace(/\r\n?/g, "\n")) {
			if (char === "\n" && mode !== "nowrap") {
				flush();
				result.push({ ...item, text: piece, break: true });
				[piece, column, started, loose] = ["", 0, false, false];
			} else if (mode === "pre-line") piece += char;
			else if (mode === "nowrap") {
				if (/[ \t\n\f]/.test(char)) {
					if (started && !loose) piece += NBSP;
					loose = started;
				} else [piece, started, loose] = [piece + char, true, false];
			} else if (char === " " || char === "\t")
				spaces += char === " " ? 1 : 8 - ((column + spaces) % 8);
			else {
				flush();
				[piece, column, started, loose] = [piece + char, column + 1, true, false];
			}
		}
		flush();
		// A collapsed space at the end of a line goes.
		if (mode === "nowrap" && loose && (item.break || index === runs.length - 1)) {
			piece = piece.slice(0, -1);
			loose = false;
		}
		if (piece || item.break) result.push({ ...item, text: piece, break: item.break });
		if (item.break) [column, started, loose] = [0, false, false];
	}
	return result;
}

/** A letter starts a word unless a letter, digit, or mark precedes it, or an apostrophe,
 *  period, colon, or middle dot inside a word, as in `don't` (Unicode word boundaries). */
const WORD_START = /(?<![\p{L}\p{N}\p{M}\p{Pc}]|[\p{L}\p{N}]['’.:·])\p{L}/gu;

/** Locale-independent, so output does not depend on the runtime's default language. */
function transformText(text: string, value: Transform, before: string): string {
	if (value === "uppercase") return text.toUpperCase();
	if (value === "lowercase") return text.toLowerCase();
	if (value === "none") return text;
	return (before + text)
		.replace(WORD_START, (letter: string, offset: number) =>
			offset < before.length ? letter : letter.toUpperCase(),
		)
		.slice(before.length);
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

/** In block flow the writer holds a box's vertical margins so they collapse with adjoining ones
 *  (CSS 2.1 8.3.1). A flex item, table cell, or row keeps its margins, and its children's, inside. */
function box(writer: ProtocolWriter, value: Box | undefined, collapses: boolean, body: () => void) {
	const [top, right, bottom, left] = value?.margin ?? [0, 0, 0, 0];
	const inner: Box | undefined =
		value && collapses ? { ...value, margin: [0, right, 0, left] } : value;
	const drawn =
		inner &&
		(!collapses ||
			inner.background ||
			[...inner.margin, ...inner.padding, ...inner.border].some(Boolean));
	if (collapses && top) writer.margin(top);
	if (drawn) writer.boxStart(inner, collapses && !inner.padding[0] && !inner.border[0]);
	try {
		body();
		if (drawn) writer.boxEnd(collapses && !inner.padding[2] && !inner.border[2]);
	} catch (error) {
		if (drawn && error instanceof Error && error.message === "PageOverflow")
			throw new Error("PageOverflow: use document margins or split this decorated box");
		throw error;
	}
	if (collapses && bottom) writer.margin(bottom);
}

/** Margins collapse in block flow, not around flex items or table cells. */
function collapses(context: Context) {
	return !context.inRow && !context.inColumn;
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
		// Whitespace between block-level children makes no line, as in CSS, unless it is preformatted.
		if (
			runs.some(
				(item) =>
					item.break ||
					!blank(item.text) ||
					(item.text && item.whiteSpace !== "normal" && item.whiteSpace !== "nowrap"),
			)
		) {
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
		// A flex item's margins, and its children's, never collapse with its siblings'.
		if (context.inColumn) writer.flush();
		compile([child], writer, fonts, context, depth, pageTokens);
		if (context.inColumn) writer.flush();
	}
	flush();
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

/** `ul` and `ol` start the counter their items number themselves from; an `li` takes the next
 *  number and, unless its type is `none`, owes its first paragraph a marker. */
function listState(node: StyledElement, next: TextState, context: TextState) {
	if (node.tag === "ul" || node.tag === "ol") {
		for (const child of node.children)
			if (child.kind === "text" ? !blank(child.value) : child.tag !== "li")
				throw new Error(`<${node.tag}> accepts only <li>`);
		const count = node.children.filter((child) => child.kind === "element").length;
		next.list = {
			next: node.start ?? (node.reversed ? count : 1),
			step: node.reversed ? -1 : 1,
			ordered: node.tag === "ol",
		};
	} else if (node.tag === "li") {
		const list = context.list;
		if (!list) throw new Error("<li> belongs in a <ul> or <ol>");
		if (node.value !== undefined && !list.ordered)
			throw new Error("value applies only to an <li> in an <ol>");
		const ordinal = node.value ?? list.next;
		list.next = ordinal + list.step;
		const type = listStyleType(node.style.listStyleType ?? "disc");
		const inside = listStylePosition(node.style.listStylePosition ?? "outside") === "inside";
		const text = SHAPES[type] ?? `${counter(type, ordinal)}. `;
		// A marker is plain text in the item's font, whatever the item inherits.
		next.item = {
			marker:
				type === "none"
					? undefined
					: {
							...run(next, text),
							underline: false,
							lineThrough: false,
							decorationColor: undefined,
							link: undefined,
							background: undefined,
							rise: 0,
							marker: inside ? 2 : 1,
							whiteSpace: "normal",
							transform: "none",
						},
		};
	}
}

/** HTML ignores one newline right after `<pre>`, so a template literal can start on its own line. */
function withoutLeadingNewline(children: readonly StyledNode[]): readonly StyledNode[] {
	const [first, ...rest] = children;
	return first?.kind === "text"
		? [{ ...first, value: first.value.replace(/^\r?\n/, "") }, ...rest]
		: children;
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
	if (style.verticalAlign !== undefined && !INLINE_TAGS.has(node.tag))
		throw new Error("vertical-align applies only to inline elements; table cells align to the top");
	switch (node.tag) {
		case "div":
		case "main":
		case "section":
		case "article":
		case "header":
		case "footer":
		case "ul":
		case "ol":
		case "li":
		case "dl":
		case "dt":
		case "dd":
		case "blockquote":
		case "address":
		case "figure":
		case "figcaption":
		case "nav":
		case "aside":
		case "td":
		case "th": {
			if ((node.tag === "td" || node.tag === "th") && !context.cell)
				throw new Error(`<${node.tag}> belongs in a <tr> inside a <table>`);
			rowOnly(style, context.inRow);
			pageBreak(writer, style, "breakBefore", depth);
			const next = inherit(style, context, fonts);
			listState(node, next, context);
			const owed = node.tag === "li" ? next.item : undefined;
			const paint = boxStyle(style, next.size);
			box(writer, paint, collapses(context), () => {
				const row = style.display === "flex" && (style.flexDirection ?? "row") === "row";
				const inner = { ...next, inRow: false, inColumn: style.display === "flex" && !row };
				const alone = () => writeParagraph(writer, [run(inner, NBSP)], inner, pageTokens);
				const first = node.children.find(
					(child) => child.kind === "element" || !blank(child.value),
				);
				if (owed?.marker && first?.kind === "element" && MARKER_ALONE.has(first.tag)) alone();
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
					writer.stackStart(gap, collapses(context));
					flow(node.children, writer, fonts, inner, depth + 1, pageTokens, 0);
					writer.stackEnd(collapses(context));
				} else flow(node.children, writer, fonts, inner, depth, pageTokens, gap);
				if (owed?.marker) alone();
			});
			pageBreak(writer, style, "breakAfter", depth);
			break;
		}
		case "a":
		case "span":
		case "b":
		case "strong":
		case "em":
		case "i":
		case "cite":
		case "small":
		case "sub":
		case "sup":
		case "u":
		case "ins":
		case "s":
		case "del":
		case "mark":
		case "code":
		case "kbd":
		case "samp":
		case "pre":
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
			const children = node.tag === "pre" ? withoutLeadingNewline(node.children) : node.children;
			const runs = collect(children, inner, fonts);
			// A link around layout nodes (an image, a card) flows them as a block; every run inside links.
			if (runs === undefined && node.tag !== "a") throw new Error(`<${node.tag}> ${INLINE_ONLY}`);
			const paint = boxStyle(style, next.size);
			if (paint && style.verticalAlign && style.verticalAlign !== "baseline")
				throw new Error(
					`vertical-align: ${style.verticalAlign} shifts text within a line, and <${node.tag}> has box styles that make it a block`,
				);
			box(writer, paint, collapses(context), () =>
				runs === undefined
					? flow(children, writer, fonts, inner, depth, pageTokens, 0)
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
			box(writer, boxStyle(style, next.size), collapses(context), () => {
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
			box(
				writer,
				boxStyle(style, inherit(style, context, fonts).size),
				collapses(context),
				() => {},
			);
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
	if (paint?.margin[0]) writer.margin(paint.margin[0]);
	const inner = { ...next, inRow: false };
	/** Element path of each table child, indexed as the renderer reports an overflowing row. */
	const wheres: string[] = [];
	const row = (tr: StyledElement, state: Context) => {
		try {
			rowOnly(tr.style, false);
			for (const key of BREAKS) pageBreak(writer, tr.style, key, depth + 1);
			const own = inherit(tr.style, state, fonts);
			box(writer, boxStyle(tr.style, own.size), false, () => {
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
	if (paint?.margin[2]) writer.margin(paint.margin[2]);
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
