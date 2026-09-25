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
		case "footer": {
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
