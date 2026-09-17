import { object, props, text } from "./assert.js";
import type { Binary } from "./binary.js";
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
	s: Record<string, unknown>,
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
	binary: Binary,
	size: number,
	value: string,
	align: string,
	paint: [number, number, number],
	fontIndex: number,
) {
	if (align === "left" && paint[0] === 0 && paint[1] === 0 && paint[2] === 0 && fontIndex === 0)
		binary.record(1, () => {
			binary.f32(size);
			binary.text(value);
		});
	else
		binary.record(16, () => {
			binary.f32(size);
			binary.u8(align === "center" ? 1 : align === "right" ? 2 : 0);
			for (const channel of paint) binary.u8(channel);
			binary.u8(fontIndex);
			binary.text(value);
		});
}

function emitBreak(
	binary: Binary,
	s: Record<string, unknown>,
	key: "breakBefore" | "breakAfter",
	depth: number,
) {
	if (s[key] !== "page") return;
	if (depth)
		throw new Error(
			`nested ${key === "breakBefore" ? "break-before" : "break-after"} is not implemented`,
		);
	binary.record(7);
}

async function withBox(
	binary: Binary,
	box: Box | undefined,
	tag: string,
	body: () => Promise<void>,
) {
	if (box)
		binary.record(17, () => {
			for (const edge of [...box.margin, ...box.padding]) binary.f32(edge);
			for (const edge of box.border) binary.f32(edge);
			if (box.background) {
				binary.u8(1);
				for (const channel of box.background) binary.u8(channel);
			} else binary.u8(0);
			for (const edge of box.borderColor) for (const channel of edge) binary.u8(channel);
		});
	try {
		await body();
		if (box) binary.record(18);
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

export async function lower(
	value: unknown,
	binary: Binary,
	fonts: ReadonlyMap<string, FontSlots>,
	inherited: Inherited = ROOT,
	depth = 0,
): Promise<void> {
	for (const child of children(value)) {
		if (typeof child === "string" || typeof child === "number") {
			emitText(
				binary,
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
				emitBreak(binary, s, "breakBefore", depth);
				const items =
					s.display === "flex" && (s.flexDirection ?? "row") === "row"
						? [...children(p.children)]
						: undefined;
				const next = nextInherited(s, inherited, fonts);
				const box = boxStyle(s, next.size);
				const streamMargin = verticalMargin(box) && !inherited.inRow && s.breakInside !== "avoid";
				if (streamMargin && box!.margin[0]) binary.record(2, () => binary.f32(box!.margin[0]));
				await withBox(binary, streamMargin ? undefined : box, node.type, async () => {
					if (items && items.length) {
						if (s.gap !== undefined && point(s.gap, next.size) !== 0)
							throw new Error("row gap is not implemented");
						binary.record(5, () => {
							binary.u16(items.length);
							for (const item of items) {
								const column = nativeColumn(item);
								binary.u8(column.kind);
								binary.f32(column.value);
							}
						});
						for (const item of items)
							await lower(item, binary, fonts, { ...next, inRow: true }, depth + 1);
						binary.record(6);
					} else if (inherited.inRow || s.breakInside === "avoid") {
						binary.record(3, () => binary.f32(s.gap === undefined ? 0 : point(s.gap, next.size)));
						await lower(p.children, binary, fonts, { ...next, inRow: false }, depth + 1);
						binary.record(4);
					} else {
						// Block children are independent renderer blocks, so normal document
						// flow can advance pages without retaining a whole <main> in memory.
						const blocks = [...children(p.children)];
						const gap = s.gap === undefined ? 0 : point(s.gap, next.size);
						for (let index = 0; index < blocks.length; index++) {
							if (index && gap) binary.record(2, () => binary.f32(gap));
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
									binary,
									next.size,
									value,
									next.align,
									next.color,
									next.bold ? next.family.bold! : next.family.regular,
								);
								continue;
							}
							await lower(blocks[index], binary, fonts, { ...next, inRow: false }, depth);
						}
					}
				});
				if (streamMargin && box!.margin[2]) binary.record(2, () => binary.f32(box!.margin[2]));
				emitBreak(binary, s, "breakAfter", depth);
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
				emitBreak(binary, s, "breakBefore", depth);
				const next = nextInherited(s, inherited, fonts);
				const box = boxStyle(s, next.size);
				await withBox(binary, box, node.type, async () => {
					emitText(
						binary,
						next.size,
						scalarText(p.children),
						next.align,
						next.color,
						next.bold ? next.family.bold! : next.family.regular,
					);
				});
				emitBreak(binary, s, "breakAfter", depth);
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
				await withBox(binary, box, node.type, async () => {});
				break;
			}
			default:
				throw new Error("unsupported element");
		}
	}
}
