import { props } from "./assert.js";
import { parse, type PageDeclaration } from "./css/parser.js";
import type { Element } from "./element.js";
import { edgeValues, point } from "./style/length.js";

export const PAGE_NUMBER = "\u001e";
export const TOTAL_PAGES = "\u001f";

/** The only way to emit a page token: the boundary rejects the raw characters in user text. */
export class PageToken {
	constructor(readonly token: string) {}
}

export function PageNumber(): Element {
	return { type: "span", props: { children: new PageToken(PAGE_NUMBER) } };
}

export function TotalPages(): Element {
	return { type: "span", props: { children: new PageToken(TOTAL_PAGES) } };
}

/** Portrait sizes in whole points, as PostScript names them, so A4 keeps its 595 x 842 output. */
const FORMATS = new Map<string, readonly [number, number]>([
	["a3", [842, 1191]],
	["a4", [595, 842]],
	["a5", [420, 595]],
	["letter", [612, 792]],
	["legal", [612, 1008]],
]);
const SIDES = ["top", "right", "bottom", "left"] as const;
type Size = [width: number, height: number];
const LONGHANDS = ["margin-top", "margin-right", "margin-bottom", "margin-left"];

/** Margins are top, right, bottom, left. */
export type PageSetup = { width: number; height: number; margin: number[] };

/** Render options win over `@page`, as an inline style wins over a stylesheet. */
export function pageSetup(
	format: unknown,
	orientation: unknown,
	margin: unknown,
	sheets: readonly string[] = [],
): PageSetup {
	const declarations: PageDeclaration[] = [];
	for (const sheet of sheets) parse(sheet, declarations);
	const css = cssPage(declarations);
	const [width, height] = turn(
		format === undefined ? (css.size ?? named("A4")) : optionSize(format),
		orientation,
	);
	const sides = margin === undefined ? css.margin.map((side) => side ?? 36) : optionMargin(margin);
	if (sides[1]! + sides[3]! >= width || sides[0]! + sides[2]! >= height)
		throw new Error("invalid margin: the margins leave no room for content");
	return { width, height, margin: sides };
}

/** Folds `@page` declarations as the cascade does: normal ones in source order, then `!important` ones. */
export function cssPage(declarations: readonly PageDeclaration[]) {
	let size: Size | undefined;
	let margin: (number | undefined)[] = [undefined, undefined, undefined, undefined];
	for (const important of [false, true])
		for (const { name, value, important: flag, where } of declarations) {
			if (flag !== important) continue;
			try {
				if (name === "size") size = cssSize(value);
				else if (name === "margin")
					margin = edgeValues(value, "margin").map((side) => length(side, "margin"));
				else margin[LONGHANDS.indexOf(name)] = length(value, name);
			} catch (error) {
				if (error instanceof Error) error.message += where;
				throw error;
			}
		}
	return { size, margin };
}

/** `size: auto | <length>{1,2} | <page-size> || [portrait | landscape]`. */
function cssSize(value: string): Size {
	const tokens = value.split(/\s+/);
	if (tokens.length <= 2 && tokens.every((token) => /^[\d.]/.test(token))) {
		const [width, height = width] = tokens.map((token) => side(token, "page size"));
		return [width!, height!];
	}
	if (value.toLowerCase() === "auto") return named("A4");
	const orientation = tokens.filter((token) => /^(?:portrait|landscape)$/i.test(token));
	const format = tokens.filter((token) => !orientation.includes(token));
	if (orientation.length > 1 || format.length > 1) throw new Error(`invalid page size: ${value}`);
	return turn(named(format[0] ?? "A4"), orientation[0]?.toLowerCase());
}

function optionSize(format: unknown): Size {
	if (typeof format !== "object" || format === null || Array.isArray(format)) return named(format);
	const { width, height } = props(format, ["width", "height"]);
	return [side(width, "page width"), side(height, "page height")];
}

function optionMargin(value: unknown): number[] {
	if (typeof value !== "object" || value === null || Array.isArray(value))
		return edgeValues(value, "margin").map((side) => length(side, "margin"));
	const sides = props(value, [...SIDES]);
	return SIDES.map((name) => length(sides[name], `margin ${name}`));
}

function named(format: unknown): Size {
	const size = typeof format === "string" ? FORMATS.get(format.toLowerCase()) : undefined;
	if (!size)
		throw new Error(
			`unsupported page size: ${String(format)} (use A3, A4, A5, Letter, Legal, or a width and height)`,
		);
	return [size[0], size[1]];
}

/** As CSS defines it: landscape puts the longer side across, portrait puts it down. */
function turn(size: Size, orientation: unknown): Size {
	if (orientation === undefined) return size;
	if (orientation !== "portrait" && orientation !== "landscape")
		throw new Error(`invalid orientation: ${String(orientation)}`);
	const [short, long] = [Math.min(...size), Math.max(...size)];
	return orientation === "landscape" ? [long, short] : [short, long];
}

function length(value: unknown, name: string): number {
	try {
		return point(value);
	} catch {
		throw new Error(`invalid ${name}: ${String(value)}`);
	}
}

/** PDF readers cap each page side at 14,400 units (ISO 32000-1, Annex C). */
function side(value: unknown, name: string): number {
	const result = length(value, name);
	if (result > 0 && result <= 14400) return result;
	throw new Error(`invalid ${name}: ${String(value)} (a page side is above 0 and at most 14400pt)`);
}
