import { object } from "./assert.js";
import { supported, unsupported } from "./css/properties.js";
import { line } from "./style/border.js";
import { decoration, decorationColor } from "./style/decoration.js";
import { bold, italic } from "./style/font.js";
import { corner, CORNERS, opacity, SIDES } from "./style/length.js";
import { dimension, flexBasis, lineHeight, point, spacing } from "./style/length.js";
import { listStylePosition, listStyleType } from "./style/list.js";
import { normalize } from "./style/normalize.js";
import type { NormalizedStyle, Size } from "./style/types.js";

export type { Box, FontSlots, NormalizedStyle, Size } from "./style/types.js";
export { boxSize, boxStyle, edges, radiusOf } from "./style/box.js";
export { color } from "./style/color.js";
export { decoration, decorationColor } from "./style/decoration.js";
export { bold, courier, face, font, helvetica, times } from "./style/font.js";
export {
	dimension,
	flexBasis,
	lineHeight,
	opacity,
	point,
	spacing,
	type LineHeight,
} from "./style/length.js";
export { counter, listStylePosition, listStyleType, SHAPES } from "./style/list.js";

export function kerning(value: unknown): boolean {
	if (value !== "auto" && value !== "normal" && value !== "none")
		throw new Error(`invalid fontKerning: ${String(value)} (use auto, normal, or none)`);
	return value !== "none";
}

const WHITE_SPACE = new Set(["normal", "nowrap", "pre", "pre-wrap", "pre-line"]);
export type WhiteSpace = "normal" | "nowrap" | "pre" | "pre-wrap" | "pre-line";
export function whiteSpace(value: unknown): WhiteSpace {
	if (typeof value !== "string" || !WHITE_SPACE.has(value))
		throw new Error(
			`invalid whiteSpace: ${String(value)} (use normal, nowrap, pre, pre-wrap, or pre-line)`,
		);
	return value as WhiteSpace;
}

/** The baseline shift of `sub` and `super` as a fraction of the parent's font size, upward. */
const SHIFTS = { baseline: 0, sub: -0.2, super: 0.33 };
export function verticalAlign(value: unknown, parent: number): number {
	if (typeof value !== "string" || !Object.hasOwn(SHIFTS, value))
		throw new Error(`invalid verticalAlign: ${String(value)} (use baseline, sub, or super)`);
	return SHIFTS[value as keyof typeof SHIFTS] * parent;
}

const TRANSFORMS = new Set(["none", "uppercase", "lowercase", "capitalize"]);
export type Transform = "none" | "uppercase" | "lowercase" | "capitalize";
export function transform(value: unknown): Transform {
	if (typeof value !== "string" || !TRANSFORMS.has(value))
		throw new Error(
			`invalid textTransform: ${String(value)} (use none, uppercase, lowercase, or capitalize)`,
		);
	return value as Transform;
}

const ALIGN = ["normal", "stretch", "flex-start", "start", "flex-end", "end", "center"];
const KEYWORDS: Record<string, readonly string[]> = {
	justifyContent: [
		"normal",
		"flex-start",
		"start",
		"flex-end",
		"end",
		"center",
		"space-between",
		"space-around",
		"space-evenly",
	],
	alignItems: ALIGN,
	alignSelf: ["auto", ...ALIGN],
	boxSizing: ["content-box", "border-box"],
};
/** Each size's initial value, which reads as absent. */
const INITIAL_SIZE = {
	width: "auto",
	height: "auto",
	minWidth: "auto",
	minHeight: "auto",
	maxWidth: "none",
	maxHeight: "none",
} as const;

/** `border-spacing` as one or two lengths; the result is true when either is nonzero. */
export function borderSpacing(value: unknown): boolean {
	const lengths = typeof value === "number" ? [value] : String(value).trim().split(/\s+/);
	if (lengths.length > 2) throw new Error(`invalid borderSpacing: ${String(value)}`);
	return lengths.map((length) => point(length)).some(Boolean);
}

/** `parent` is the parent element's normalized style, which `inherit` reads. */
export function style(value: unknown, parent: NormalizedStyle = {}): NormalizedStyle {
	if (value === undefined) return {};
	const source = object(value);
	for (const key of Object.keys(source)) {
		if (key.startsWith("--")) continue;
		const reason = unsupported[key as keyof typeof unsupported];
		if (reason) throw new Error(`unsupported style property: ${key} (${reason})`);
		if (!supported.has(key as never)) throw new Error(`unsupported style property: ${key}`);
	}
	// Validated after normalize, so `inherit` and `initial` have become values.
	const result = normalize(source, parent) as Record<string, unknown>;
	if (result.display !== undefined && result.display !== "block" && result.display !== "flex")
		throw new Error("invalid display");
	if (
		result.flexDirection !== undefined &&
		result.flexDirection !== "row" &&
		result.flexDirection !== "column"
	)
		throw new Error("invalid flex direction");
	if (
		result.textAlign !== undefined &&
		result.textAlign !== "left" &&
		result.textAlign !== "center" &&
		result.textAlign !== "right" &&
		result.textAlign !== "justify"
	)
		throw new Error("invalid text alignment");
	for (const key of ["breakBefore", "breakAfter", "breakInside"] as const) {
		const value = result[key] as string | undefined;
		const allowed = key === "breakInside" ? ["auto", "avoid"] : ["auto", "avoid", "page"];
		// Without columns or regions, avoiding a page break is avoiding any break.
		if (value === "avoid-page") result[key] = "avoid";
		else if (key !== "breakInside" && ["left", "right", "recto", "verso"].includes(value!))
			throw new Error(`${key}: ${value} is not supported (pages have no sides; use page)`);
		else if (value !== undefined && !allowed.includes(value)) throw new Error(`invalid ${key}`);
	}
	for (const key of ["orphans", "widows"] as const) {
		const value =
			typeof result[key] === "string" && /^\s*\d+\s*$/.test(result[key])
				? Number(result[key])
				: result[key];
		if (value === undefined) continue;
		if (!Number.isInteger(value) || (value as number) < 1)
			throw new Error(`invalid ${key}: ${String(value)} (use a whole number of lines from 1)`);
		// No paragraph record holds 65,535 lines, so the cap changes nothing.
		result[key] = Math.min(value as number, 0xffff);
	}
	if (
		result.borderCollapse !== undefined &&
		result.borderCollapse !== "collapse" &&
		result.borderCollapse !== "separate"
	)
		throw new Error("invalid borderCollapse");
	if (result.borderSpacing !== undefined) borderSpacing(result.borderSpacing);
	if (result.flexWrap !== undefined && result.flexWrap !== "nowrap")
		throw new Error(
			`flex-wrap: ${String(result.flexWrap)} is not supported; flex items stay on one line, so size them to fit`,
		);
	delete result.flexWrap;
	for (const [key, values] of Object.entries(KEYWORDS))
		if (result[key] !== undefined && !values.includes(result[key] as string))
			throw new Error(
				/baseline/.test(String(result[key]))
					? `${key}: ${String(result[key])} is not supported; align text with center or flex-start`
					: `invalid ${key}: ${String(result[key])}`,
			);
	for (const key of ["flexGrow", "flexShrink"])
		if (result[key] !== undefined) {
			const factor = typeof result[key] === "string" ? Number(result[key]) : result[key];
			if (typeof factor !== "number" || !(factor >= 0) || !Number.isFinite(Math.fround(factor)))
				throw new Error(`invalid ${key}: ${String(result[key])}`);
			result[key] = Math.fround(factor);
		}
	if (result.flexBasis !== undefined) flexBasis(result.flexBasis);
	for (const [key, initial] of Object.entries(INITIAL_SIZE)) {
		if (result[key] === initial) delete result[key];
		else if (result[key] !== undefined) dimension(result[key], key, true);
	}
	if (result.gap !== undefined) point(result.gap);
	if (result.fontSize !== undefined) point(result.fontSize, 12, true);
	if (result.fontWeight !== undefined) bold(result.fontWeight);
	if (result.fontStyle !== undefined) italic(result.fontStyle);
	if (result.fontKerning !== undefined) kerning(result.fontKerning);
	if (result.letterSpacing !== undefined) spacing(result.letterSpacing, 12, "letterSpacing");
	if (result.wordSpacing !== undefined) spacing(result.wordSpacing, 12, "wordSpacing");
	if (result.textTransform !== undefined) transform(result.textTransform);
	if (result.textDecorationColor !== undefined) decorationColor(result.textDecorationColor);
	if (result.whiteSpace !== undefined) whiteSpace(result.whiteSpace);
	if (
		result.verticalAlign !== undefined &&
		!["top", "middle", "bottom"].includes(String(result.verticalAlign))
	)
		verticalAlign(result.verticalAlign, 12);
	if (result.listStyleType !== undefined) listStyleType(result.listStyleType);
	if (result.listStylePosition !== undefined) listStylePosition(result.listStylePosition);
	if (result.lineHeight !== undefined) lineHeight(result.lineHeight, 12);
	const family = result.fontFamily;
	if (family !== undefined && (typeof family !== "string" || !family.trim()))
		throw new Error("fontFamily must name at least one family");
	if (result.textDecoration !== undefined) decoration(result.textDecoration);
	if (result.opacity !== undefined) opacity(result.opacity);
	for (const side of SIDES)
		if (result[`border${side}Style`] !== undefined)
			result[`border${side}Style`] = line(result[`border${side}Style`]);
	for (const name of CORNERS)
		if (result[`border${name}Radius`] !== undefined) corner(result[`border${name}Radius`], 12);
	if (
		typeof result.backgroundColor === "string" &&
		/(?:gradient|url)\(/i.test(result.backgroundColor)
	)
		throw new Error(
			`unsupported background: ${result.backgroundColor} (gradients and background images are not drawn; use a solid colour)`,
		);
	return result as NormalizedStyle;
}
