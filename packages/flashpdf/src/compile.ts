import type { StyledNode } from "./css.js";
import type { StyledElement } from "./css/cascade.js";
import { PAGE_NUMBER, TOTAL_PAGES } from "./page.js";
import type { ProtocolWriter, Column } from "./protocol.js";
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

function writeText(writer: ProtocolWriter, value: string, state: TextState, pageTokens: boolean) {
	if (!pageTokens && (value.includes(PAGE_NUMBER) || value.includes(TOTAL_PAGES)))
		throw new Error("page tokens are only valid in a footer");
	writer.text(
		value,
		state.size,
		state.align,
		state.color,
		state.bold ? state.family.bold! : state.family.regular,
	);
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

function textOnly(children: readonly StyledNode[]): string {
	let value = "";
	for (const child of children) {
		if (child.kind !== "text") throw new Error("text elements only accept text children");
		value += child.value;
	}
	return value;
}

/** Text of a span that changes nothing about how its text paints, so it can join the parent's run. */
function inlineText(value: StyledNode, inherited: Context, fonts: Fonts): string | undefined {
	if (value.kind === "text") return value.value;
	if (value.tag !== "span") return undefined;
	try {
		const style = value.style;
		rowOnly(style, inherited.inRow);
		if (
			boxStyle(style, inherited.size) ||
			style.breakBefore === "page" ||
			style.breakAfter === "page"
		)
			return undefined;
		const next = inherit(style, inherited, fonts);
		if (
			next.size !== inherited.size ||
			next.align !== inherited.align ||
			next.bold !== inherited.bold ||
			next.family !== inherited.family ||
			next.color.some((channel, index) => channel !== inherited.color[index])
		)
			return undefined;
		let result = "";
		for (const child of value.children) {
			const text = inlineText(child, inherited, fonts);
			if (text === undefined) return undefined;
			result += text;
		}
		return result;
	} catch (error) {
		throw locate(error, value.where);
	}
}

/** Merges plain inline children from `start` into one text run; undefined when `start` is a layout node. */
function inlineRun(
	children: readonly StyledNode[],
	start: number,
	context: Context,
	fonts: Fonts,
): { text: string; end: number } | undefined {
	let text = "";
	let end = start;
	for (; end < children.length; end++) {
		const part = inlineText(children[end]!, context, fonts);
		if (part === undefined) break;
		text += part;
	}
	return end === start ? undefined : { text, end };
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
				if (row && node.children.length) {
					if (style.gap !== undefined && point(style.gap, next.size) !== 0)
						throw new Error("row gap is not implemented");
					writer.rowStart(node.children.map(column));
					compile(node.children, writer, fonts, { ...next, inRow: true }, depth + 1, pageTokens);
					writer.rowEnd();
				} else if (context.inRow || style.breakInside === "avoid") {
					writer.stackStart(style.gap === undefined ? 0 : point(style.gap, next.size));
					compile(node.children, writer, fonts, inner, depth + 1, pageTokens);
					writer.stackEnd();
				} else {
					const gap = style.gap === undefined ? 0 : point(style.gap, next.size);
					for (let index = 0; index < node.children.length;) {
						if (index && gap) writer.spacer(gap);
						const run = inlineRun(node.children, index, inner, fonts);
						if (run) {
							writeText(writer, run.text, next, pageTokens);
							index = run.end;
						} else {
							compile([node.children[index]!], writer, fonts, inner, depth, pageTokens);
							index++;
						}
					}
				}
			});
			if (streamed && paint?.margin[2]) writer.spacer(paint.margin[2]);
			pageBreak(writer, style, "breakAfter", depth);
			break;
		}
		case "span":
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
			box(writer, boxStyle(style, next.size), () =>
				writeText(writer, textOnly(node.children), next, pageTokens),
			);
			pageBreak(writer, style, "breakAfter", depth);
			break;
		}
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
			writeText(writer, node.value, context, pageTokens);
			continue;
		}
		try {
			element(node, writer, fonts, context, depth, pageTokens);
		} catch (error) {
			throw locate(error, node.where);
		}
	}
}
