import type { StyledNode } from "./css.js";
import type { StyledElement } from "./css/cascade.js";
import { inherited } from "./css/properties.js";
import { glyphError } from "./fonts.js";
import { PAGE_NUMBER, TOTAL_PAGES } from "./page.js";
import type { ProtocolWriter, Column, FlexItem, Run, Slot } from "./protocol.js";
import {
	borderSpacing,
	boxSize,
	boxStyle,
	color,
	counter,
	decoration,
	decorationColor,
	dimension,
	face,
	flexBasis,
	font,
	helvetica,
	kerning,
	lineHeight,
	opacity,
	point,
	radiusOf,
	listStylePosition,
	listStyleType,
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
import { encode, frame } from "./svg.js";
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
	/** Not inherited by CSS, but each element paints its own, so the product down the tree is the effective value. */
	opacity: number;
	/** Set only while compiling the document body. */
	nav: Navigation | undefined;
	/** Set on the direct children of a `ul` or `ol`, whose items number themselves from it. */
	list: { next: number; step: number; ordered: boolean } | undefined;
	/** The list item being compiled, which its first paragraph empties by taking the marker. */
	markerState: { marker: Run | undefined } | undefined;
	orphans: number;
	widows: number;
};
/** A cell's collapsed borders, and its row's background when rowSpan joins the row to others. */
type CellPaint = Partial<Pick<Box, "border" | "borderColor" | "background">>;
/** `cell` is set only for the direct children of a table row. */
type Context = TextState & {
	inRow: boolean;
	cell?: CellPaint;
	item?: boolean;
	inColumn?: boolean;
	height?: number | undefined;
};
type Fonts = ReadonlyMap<string, FontSlots>;
/** What a document needs before its first record: the ids links name, and whether to bookmark headings. */
type Navigation = {
	/** Each id a link names, with its element. */
	targets: ReadonlyMap<string, StyledElement>;
	outline: boolean;
	/** The targets whose anchor record is written. */
	anchored: Set<string>;
	/** Anchor records written, against the engine's cap. */
	count: number;
};
/** Destinations and bookmarks per render, matching the engine's cap. */
const MAX_ANCHORS = 10_000;
const MAX_TITLE = 255;

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
	opacity: 1,
	nav: undefined,
	list: undefined,
	markerState: undefined,
	orphans: 2,
	widows: 2,
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
const VERTICAL_ALIGN =
	"vertical-align applies only to inline elements and <td>, <th>, <tr>, and row groups";
const TABLE_PARTS = new Set<string>(["td", "th", "tr", "thead", "tbody", "tfoot"]);
/** Tags whose `display: flex` lays out their children. */
const BLOCK_TAGS = new Set<string>([
	"div",
	"main",
	"section",
	"article",
	"header",
	"footer",
	"nav",
	"aside",
	"address",
	"blockquote",
	"figure",
	"figcaption",
	"ul",
	"ol",
	"li",
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
	borderStyle: [0, 0, 0, 0],
	opacity: 1,
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
const FLEX_ALIGN: Record<string, number> = {
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
		"TextTooWide: a character is wider than its column; reduce the font size or widen the column",
	],
	[
		"ContentOverflow",
		"ContentOverflow: the content is taller than its box's height or max-height, or than its flex container; raise the height or shorten the content",
	],
	[
		"LineTooWide",
		"LineTooWide: a line of white-space: pre or nowrap text is wider than its container; shorten it, reduce the font size, or use white-space: pre-wrap",
	],
]);
/** Explains the engine's bare error codes; `locate` then names the element. */
export function engineError(error: unknown): unknown {
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
			(style.verticalAlign === undefined ||
			["top", "middle", "bottom"].includes(style.verticalAlign)
				? 0
				: verticalAlign(style.verticalAlign, current.size)),
		background: current.background,
		link: current.link,
		opacity:
			style.opacity === undefined ? current.opacity : current.opacity * opacity(style.opacity),
		nav: current.nav,
		list: undefined,
		markerState: current.markerState,
		orphans: style.orphans ?? current.orphans,
		widows: style.widows ?? current.widows,
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
		opacity: state.opacity,
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
		last.opacity === next.opacity &&
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
		if (
			style.verticalAlign !== undefined &&
			["top", "middle", "bottom"].includes(style.verticalAlign)
		)
			verticalAlign(style.verticalAlign, context.size);
		rowOnly(style, context.inRow);
		// A `<mark>` paints its background behind each line of its text, so only box edges make it a box.
		const paint = boxStyle(style, context.size);
		const highlight =
			node.tag === "mark" &&
			paint?.margin.concat(paint.padding, paint.border).every((edge) => !edge);
		if (
			(paint && !highlight) ||
			sized(style) ||
			style.breakBefore === "page" ||
			style.breakAfter === "page"
		)
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
	if (state.markerState?.marker) {
		shown.unshift(state.markerState.marker);
		state.markerState.marker = undefined;
	}
	writer.paragraph(shown, state.align, state.orphans, state.widows);
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

/** Inside a flex row or a kept-together group every break is avoided already, so the engine ignores `avoid` there. */
function pageBreak(
	writer: ProtocolWriter,
	style: NormalizedStyle,
	key: "breakBefore" | "breakAfter",
	depth: number,
) {
	if (style[key] === "avoid") writer.avoid();
	if (style[key] !== "page") return;
	if (depth)
		throw new Error(
			`nested ${key === "breakBefore" ? "break-before" : "break-after"} is not implemented`,
		);
	writer.pageBreak();
}

/** In block flow the writer holds a box's vertical margins so they collapse with adjoining ones
 *  (CSS 2.1 8.3.1). A flex item, table cell, or row keeps its margins, and its children's, inside. */
function box(
	writer: ProtocolWriter,
	value: Box | undefined,
	collapses: boolean,
	body: () => void,
	avoid = false,
) {
	const [top, right, bottom, left] = value?.margin ?? [0, 0, 0, 0];
	const inner: Box | undefined =
		value && collapses ? { ...value, margin: [0, right, 0, left] } : value;
	const drawn =
		inner &&
		(!collapses ||
			inner.background ||
			inner.size ||
			[...inner.margin, ...inner.padding, ...inner.border].some(Boolean));
	if (collapses && top) writer.margin(top);
	if (drawn)
		writer.boxStart(inner, collapses && !inner.size && !inner.padding[0] && !inner.border[0]);
	try {
		body();
		if (drawn) writer.boxEnd(collapses && !inner.padding[2] && !inner.border[2]);
	} catch (error) {
		if (avoid && error instanceof Error && error.message === "PageOverflow")
			throw new Error("PageOverflow: the box is taller than a page; drop break-inside: avoid");
		if (drawn && error instanceof Error && error.message === "PageOverflow")
			throw new Error(
				(value?.size?.height ?? value?.size?.maxHeight) === undefined
					? "PageOverflow: use document margins or split this decorated box"
					: "PageOverflow: a box with a height or max-height moves whole, and this one is taller than a page",
			);
		throw error;
	}
	if (collapses && bottom) writer.margin(bottom);
}

/** Margins collapse in block flow, not around flex items or table cells. */
function collapses(context: Context) {
	return !context.inRow && !context.inColumn && !context.item && !context.cell;
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
	const paint = boxStyle(
		style,
		size,
		context.opacity * (style.opacity === undefined ? 1 : opacity(style.opacity)),
	);
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

function cellBox(box: Box | undefined, cell: CellPaint | undefined): Box | undefined {
	if (!cell?.border && !cell?.background) return box;
	const base: Box = box ?? {
		margin: [0, 0, 0, 0],
		padding: [0, 0, 0, 0],
		border: [0, 0, 0, 0],
		borderColor: [BLACK, BLACK, BLACK, BLACK],
		borderStyle: [0, 0, 0, 0],
		opacity: 1,
	};
	return { ...base, ...cell, background: base.background ?? cell.background };
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
			align: FLEX_ALIGN[s.alignSelf ?? "auto"]!,
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
			align: FLEX_ALIGN[style.alignItems ?? "normal"]!,
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
		next.markerState = {
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
	anchor(writer, node, context.nav);
	if (style.verticalAlign !== undefined && !TABLE_PARTS.has(node.tag) && !INLINE_TAGS.has(node.tag))
		throw new Error(VERTICAL_ALIGN);
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
			if (context.cell) sizeless(style, node.tag, ["width"]);
			rowOnly(style, context.inRow);
			pageBreak(writer, style, "breakBefore", depth);
			const next = inherit(style, context, fonts);
			listState(node, next, context);
			const owed = node.tag === "li" ? next.markerState : undefined;
			const paint = cellBox(layoutBox(style, next.size, context), context.cell);
			// A CSS flex item is one element, so one without a box of its own gets an empty one.
			box(
				writer,
				paint ?? (context.item ? EMPTY : undefined),
				collapses(context),
				() => {
					const flex = style.display === "flex";
					const row = flex && (style.flexDirection ?? "row") === "row";
					const [minHeight, maxHeight] = heights(paint);
					const inner = {
						...next,
						inRow: false,
						height: minHeight === maxHeight ? minHeight : undefined,
						inColumn: flex && !row,
					};
					const alone = () => writeParagraph(writer, [run(inner, NBSP)], inner, pageTokens);
					const first = node.children.find(
						(child) => child.kind === "element" || !blank(child.value),
					);
					if (owed?.marker && first?.kind === "element" && MARKER_ALONE.has(first.tag)) alone();
					const gap = style.gap === undefined ? 0 : point(style.gap, next.size);
					// A whitespace-only text child is no flex item, as in CSS.
					const items = node.children.filter(
						(child) => child.kind !== "text" || !blank(child.value),
					);
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
						writer.stackStart(gap, collapses(context));
						flow(node.children, writer, fonts, inner, depth + 1, pageTokens, 0);
						writer.stackEnd(collapses(context));
					} else flow(node.children, writer, fonts, inner, depth, pageTokens, gap);
					if (owed?.marker) alone();
				},
				style.breakInside === "avoid",
			);
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
			const paint = boxStyle(style, next.size, next.opacity);
			if (paint && style.verticalAlign && style.verticalAlign !== "baseline")
				throw new Error(
					`vertical-align: ${style.verticalAlign} shifts text within a line, and <${node.tag}> has box styles that make it a block`,
				);
			const own = layoutBox(style, next.size, context);
			const avoid = style.breakInside === "avoid";
			box(
				writer,
				own ?? (context.item && runs === undefined ? EMPTY : undefined),
				collapses(context),
				() => {
					if (avoid) writer.stackStart(0);
					if (runs === undefined)
						flow(node.children, writer, fonts, inner, avoid ? depth + 1 : depth, pageTokens, 0);
					else writeParagraph(writer, runs, next, pageTokens);
					if (avoid) writer.stackEnd();
				},
				avoid,
			);
			pageBreak(writer, style, "breakAfter", depth);
			break;
		}
		case "img":
		case "svg": {
			rowOnly(style, context.inRow);
			sizeless(style, node.tag, ["width", "height", "maxWidth", "maxHeight"]);
			for (const key of ["height", "maxHeight"] as const) {
				const value = style[key];
				if (typeof value === "string" && value.endsWith("%"))
					throw new Error(`percent ${key} has no container height; use pt or px, or set width`);
			}
			pageBreak(writer, style, "breakBefore", depth);
			const next = inherit(style, context, fonts);
			const width = style.width === undefined ? undefined : dimension(style.width, "width");
			if (context.item && width?.kind === 2)
				throw new Error(
					"an image flex item takes a width in pt or px; wrap it to size it by percent",
				);
			const height =
				style.height === undefined ? undefined : dimension(style.height, "height").value;
			const maxWidth =
				style.maxWidth === undefined ? undefined : dimension(style.maxWidth, "maxWidth");
			if (context.item && maxWidth?.kind === 2)
				throw new Error(
					"an image flex item takes a maxWidth in pt or px; wrap it to size it by percent",
				);
			const maxHeight =
				style.maxHeight === undefined ? undefined : dimension(style.maxHeight, "maxHeight").value;
			if (radiusOf(style, next.size))
				throw new Error(
					`border-radius does not clip <${node.tag}>: the picture stays rectangular, so remove the radius`,
				);
			box(writer, boxStyle(style, next.size, next.opacity), collapses(context), () => {
				try {
					const source = node.drawing
						? writer.drawing(
								encode(node.drawing, frame(node.drawing, width, height), next.color, next.opacity),
							)
						: node.src!;
					// A drawing carries its opacity in its shapes, where an image takes it from the page.
					writer.image(
						source,
						{ width, height, maxWidth, maxHeight },
						node.alt ?? "",
						context.link,
						node.drawing ? 1 : next.opacity,
					);
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
				layoutBox(style, inherit(style, context, fonts).size, context),
				collapses(context),
				() => {},
			);
			pageBreak(writer, style, "breakAfter", depth);
			break;
	}
}

function plain(nodes: readonly StyledNode[]): string {
	return nodes
		.map((node) =>
			node.kind === "text" ? node.value : node.tag === "br" ? " " : plain(node.children),
		)
		.join("");
}

/** Writes the destination that a link to `node.id` or the outline needs, before the node's content.
 *  The engine places it at the top of the next line, so it moves with the node across a page break. */
function anchor(writer: ProtocolWriter, node: StyledElement, nav: Navigation | undefined) {
	if (!nav) return;
	const named = node.id !== undefined && nav.targets.has(node.id);
	const heading = nav.outline && node.bookmark !== false && /^h([1-6])$/.exec(node.tag);
	const level = heading ? Number(heading[1]) : 0;
	const title = !level
		? ""
		: typeof node.bookmark === "string"
			? node.bookmark
			: [...plain(node.children).replace(/\s+/g, " ").trim()].slice(0, MAX_TITLE).join("");
	if (!named && !title) return;
	if (++nav.count > MAX_ANCHORS)
		throw new Error(`more than ${MAX_ANCHORS} link targets and bookmarks`);
	writer.anchor(named ? node.id! : "", title ? level : 0, title);
	if (named) nav.anchored.add(node.id!);
}

/** The ids that `<a href="#id">` names, each with its element. Every one must be on exactly one element. */
function linkTargets(nodes: readonly StyledNode[]): Map<string, StyledElement> {
	const links = new Map<string, StyledElement>();
	const owners = new Map<string, StyledElement>();
	const twins = new Map<string, StyledElement>();
	const walk = (list: readonly StyledNode[]) => {
		for (const node of list) {
			if (node.kind === "text") continue;
			if (node.href?.startsWith("#") && !links.has(node.href.slice(1)))
				links.set(node.href.slice(1), node);
			if (node.id !== undefined)
				if (owners.has(node.id)) twins.set(node.id, node);
				else owners.set(node.id, node);
			walk(node.children);
		}
	};
	walk(nodes);
	const targets = new Map<string, StyledElement>();
	for (const [id, link] of links) {
		const owner = owners.get(id);
		if (!owner) throw locate(new Error(`link to #${id}: no element has id="${id}"`), link.where);
		const twin = twins.get(id);
		if (twin)
			throw locate(new Error(`link to #${id}: id="${id}" is on more than one element`), twin.where);
		targets.set(id, owner);
	}
	return targets;
}

/** Compiles the document body. A link target or a bookmarked heading gets an `anchor` record. */
export function compileBody(
	nodes: readonly StyledNode[],
	writer: ProtocolWriter,
	fonts: Fonts,
	outline: boolean,
) {
	const nav: Navigation = { targets: linkTargets(nodes), outline, anchored: new Set(), count: 0 };
	compile(nodes, writer, fonts, { ...ROOT, nav });
	for (const [id, owner] of nav.targets)
		if (!nav.anchored.has(id))
			throw locate(
				new Error(
					`link to #${id}: its element cannot hold a destination; put the id on a block, heading, paragraph, image, table, or table cell`,
				),
				owner.where,
			);
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
	if ((style.flexGrow ?? style.flexShrink ?? style.flexBasis) !== undefined)
		throw new Error("flex applies only to a flex row child");
	sizeless(style, "table", ["width"]);
	if (context.item && style.width !== undefined)
		throw new Error("a table in a flex container takes no width; set it on a wrapping <div>");
	pageBreak(writer, style, "breakBefore", depth);
	const next = inherit(style, context, fonts);
	const paint = boxStyle(style, next.size, next.opacity);
	if (
		paint &&
		(paint.margin[1] ||
			paint.margin[3] ||
			paint.background ||
			[...paint.padding, ...paint.border].some(Boolean))
	)
		throw new Error("<table> accepts only vertical margins; style its cells or a wrapping <div>");
	const collapsed = style.borderCollapse === "collapse";
	if (!collapsed && style.borderSpacing !== undefined && borderSpacing(style.borderSpacing))
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
				if (group.style.breakBefore === "page" || group.style.breakAfter === "page")
					throw new Error("page breaks inside tables are not supported; split the table");
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
				if (tr.style.breakBefore === "page" || tr.style.breakAfter === "page")
					throw new Error("page breaks inside tables are not supported; split the table");
				const own = { ...inherit(tr.style, state, fonts), inRow: false };
				const row: GridRow = {
					node: tr,
					state: own,
					paint: boxStyle(tr.style, own.size, own.opacity),
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
					const vertical =
						cell.style.verticalAlign ??
						tr.style.verticalAlign ??
						group?.style.verticalAlign ??
						"top";
					if (vertical !== "top" && vertical !== "middle" && vertical !== "bottom")
						throw locate(
							new Error(
								"vertical-align: baseline is not implemented in tables; use top, middle, or bottom",
							),
							cell.where,
						);
					const align = ALIGN[vertical];
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
			if (item.node.style.breakBefore === "avoid") writer.avoid();
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
			box(writer, joined || !(shaped || own?.background) ? undefined : own, false, () => {
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
			if (item.node.style.breakAfter === "avoid") writer.avoid();
		} catch (error) {
			throw locate(error, item.node.where);
		}
	};
	const group = ({ group, rows }: GridGroup, repeated: boolean) => {
		if (!repeated && group?.style.breakBefore === "avoid") writer.avoid();
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
		if (!repeated && group?.style.breakAfter === "avoid") writer.avoid();
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
