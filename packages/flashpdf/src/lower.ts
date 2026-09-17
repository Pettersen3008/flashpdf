import { object, props, text } from "./assert.js";
import type { ProtocolWriter } from "./protocol.js";
import {
	style,
	point,
	color,
	boxStyle,
	font,
	nativeColumn,
	helvetica,
	type Box,
	type FontSlots,
	type NormalizedStyle,
} from "./style.js";
import { children, scalarText, rowOnly } from "./tree.js";

type StyleContext = {
	size: number;
	align: string;
	color: [number, number, number];
	family: FontSlots;
	bold: boolean;
};
export type Inherited = StyleContext & { inRow: boolean };
const ROOT: Inherited = {
	size: 12,
	align: "left",
	color: [0, 0, 0],
	family: helvetica,
	bold: false,
	inRow: false,
};

/** Resolves a child's inherited text/font state once; the flex-row, avoid-break
 *  wrapper, block-flow, and leaf-text branches below all share this. */
function nextInherited(
	s: NormalizedStyle,
	current: Inherited,
	fonts: ReadonlyMap<string, FontSlots>,
): StyleContext {
	const selected = font(s, current.family, current.bold, fonts);
	return {
		size: s.fontSize === undefined ? current.size : point(s.fontSize, current.size, true),
		align: typeof s.textAlign === "string" ? s.textAlign : current.align,
		color: s.color === undefined ? current.color : color(s.color),
		family: selected.family,
		bold: selected.weight,
	};
}

function emitText(
	writer: ProtocolWriter,
	size: number,
	value: string,
	align: string,
	paint: [number, number, number],
	fontIndex: number,
) {
	writer.text({ value, size, align, color: paint, font: fontIndex });
}

function emitBreak(
	writer: ProtocolWriter,
	s: NormalizedStyle,
	key: "breakBefore" | "breakAfter",
	depth: number,
) {
	if (s[key] !== "page") return;
	if (depth)
		throw new Error(
			`nested ${key === "breakBefore" ? "break-before" : "break-after"} is not implemented`,
		);
	writer.pageBreak();
}

function withBox(writer: ProtocolWriter, box: Box | undefined, tag: string, body: () => void) {
	if (box) writer.boxStart(box);
	try {
		body();
		if (box) writer.boxEnd();
	} catch (error) {
		const message = error instanceof Error ? error.message : String(error);
		if (box && message === "PageOverflow")
			throw new Error(`PageOverflow in <${tag}>: use document margins or split this decorated box`);
		throw error;
	}
}

function inlineText(
	value: unknown,
	inherited: Inherited,
	fonts: ReadonlyMap<string, FontSlots>,
): string | undefined {
	if (typeof value === "string" || typeof value === "number") return text(value);
	if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
	const node = object(value);
	if (node.type !== "span") return undefined;
	const p = props(node.props, ["children", "style"]);
	const s = style(p.style, "span");
	rowOnly(s, "span", inherited.inRow);
	if (boxStyle(s, inherited.size) || s.breakBefore === "page" || s.breakAfter === "page")
		return undefined;
	const next = nextInherited(s, inherited, fonts);
	if (
		next.size !== inherited.size ||
		next.align !== inherited.align ||
		next.bold !== inherited.bold ||
		next.family !== inherited.family ||
		next.color.some((channel, index) => channel !== inherited.color[index])
	)
		return undefined;
	let result = "";
	for (const child of children(p.children)) {
		const nested = inlineText(child, inherited, fonts);
		if (nested === undefined) return undefined;
		result += nested;
	}
	return result;
}

function verticalMargin(box: Box | undefined) {
	return (
		box &&
		box.margin[1] === 0 &&
		box.margin[3] === 0 &&
		box.padding.every((edge) => edge === 0) &&
		box.border.every((edge) => edge === 0) &&
		box.background === undefined
	);
}

export function lower(
	value: unknown,
	writer: ProtocolWriter,
	fonts: ReadonlyMap<string, FontSlots>,
	inherited: Inherited = ROOT,
	depth = 0,
): void {
	for (const child of children(value)) {
		if (typeof child === "string" || typeof child === "number") {
			emitText(
				writer,
				inherited.size,
				text(child),
				inherited.align,
				inherited.color,
				inherited.bold ? inherited.family.bold! : inherited.family.regular,
			);
			continue;
		}
		const node = object(child);
		switch (node.type) {
			case "div":
			case "main":
			case "section":
			case "article":
			case "header":
			case "footer": {
				const p = props(node.props, ["children", "style"]);
				const s = style(p.style, node.type);
				rowOnly(s, node.type, inherited.inRow);
				emitBreak(writer, s, "breakBefore", depth);
				const items =
					s.display === "flex" && (s.flexDirection ?? "row") === "row"
						? [...children(p.children)]
						: undefined;
				const next = nextInherited(s, inherited, fonts);
				const box = boxStyle(s, next.size);
				const streamMargin = verticalMargin(box) && !inherited.inRow && s.breakInside !== "avoid";
				if (streamMargin && box!.margin[0]) writer.spacer(box!.margin[0]);
				withBox(writer, streamMargin ? undefined : box, node.type, () => {
					if (items && items.length) {
						if (s.gap !== undefined && point(s.gap, next.size) !== 0)
							throw new Error("row gap is not implemented");
						writer.rowStart(items.map(nativeColumn));
						for (const item of items)
							lower(item, writer, fonts, { ...next, inRow: true }, depth + 1);
						writer.rowEnd();
					} else if (inherited.inRow || s.breakInside === "avoid") {
						writer.stackStart(s.gap === undefined ? 0 : point(s.gap, next.size));
						lower(p.children, writer, fonts, { ...next, inRow: false }, depth + 1);
						writer.stackEnd();
					} else {
						// Block children are independent renderer blocks, so normal document
						// flow can advance pages without retaining a whole <main> in memory.
						const blocks = [...children(p.children)];
						const gap = s.gap === undefined ? 0 : point(s.gap, next.size);
						for (let index = 0; index < blocks.length; index++) {
							if (index && gap) writer.spacer(gap);
							const inline = inlineText(blocks[index], { ...next, inRow: false }, fonts);
							if (inline !== undefined) {
								let value = inline;
								while (++index < blocks.length) {
									const sibling = inlineText(blocks[index], { ...next, inRow: false }, fonts);
									if (sibling === undefined) break;
									value += sibling;
								}
								if (index < blocks.length) index--;
								emitText(
									writer,
									next.size,
									value,
									next.align,
									next.color,
									next.bold ? next.family.bold! : next.family.regular,
								);
								continue;
							}
							lower(blocks[index], writer, fonts, { ...next, inRow: false }, depth);
						}
					}
				});
				if (streamMargin && box!.margin[2]) writer.spacer(box!.margin[2]);
				emitBreak(writer, s, "breakAfter", depth);
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
				const p = props(node.props, ["children", "style"]);
				const s = style(p.style, node.type);
				rowOnly(s, node.type, inherited.inRow);
				emitBreak(writer, s, "breakBefore", depth);
				const next = nextInherited(s, inherited, fonts);
				const box = boxStyle(s, next.size);
				withBox(writer, box, node.type, () => {
					emitText(
						writer,
						next.size,
						scalarText(p.children),
						next.align,
						next.color,
						next.bold ? next.family.bold! : next.family.regular,
					);
				});
				emitBreak(writer, s, "breakAfter", depth);
				break;
			}
			case "hr": {
				const p = props(node.props, ["children", "style"]);
				const s = style(p.style, node.type);
				const next = nextInherited(s, inherited, fonts);
				const rule =
					s.borderWidth === undefined &&
					s.borderBottomWidth === undefined &&
					s.borderBottom === undefined;
				const box = boxStyle(
					style(rule ? { ...s, borderBottom: "1pt solid black" } : s),
					next.size,
				)!;
				withBox(writer, box, node.type, () => {});
				break;
			}
			default:
				throw new Error("unsupported element");
		}
	}
}
