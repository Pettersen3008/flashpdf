import { object } from "./assert.js";
import { supported, unsupported } from "./css/properties.js";
import { decoration, decorationColor } from "./style/decoration.js";
import { bold, italic } from "./style/font.js";
import { dimension, flexGrow, lineHeight, point, spacing } from "./style/length.js";
import { listStylePosition, listStyleType } from "./style/list.js";
import { normalize } from "./style/normalize.js";
import type { NormalizedStyle } from "./style/types.js";

export type { Box, FontSlots, NormalizedStyle } from "./style/types.js";
export { boxStyle, edges } from "./style/box.js";
export { color } from "./style/color.js";
export { decoration, decorationColor } from "./style/decoration.js";
export { bold, courier, face, font, helvetica, times } from "./style/font.js";
export { dimension, lineHeight, point, spacing, type LineHeight } from "./style/length.js";
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
	for (const key of ["breakBefore", "breakAfter"] as const)
		if (result[key] !== undefined && result[key] !== "auto" && result[key] !== "page")
			throw new Error(`invalid ${key}`);
	if (
		result.breakInside !== undefined &&
		result.breakInside !== "auto" &&
		result.breakInside !== "avoid"
	)
		throw new Error("invalid breakInside");
	if (result.flex !== undefined) {
		const { grow, basis } = flexGrow(result.flex);
		// A column keeps only the grow factor, so a length basis, or `auto` beside a width, would be dropped.
		if (grow > 0 && (basis === "length" || (basis === "auto" && result.width !== undefined)))
			throw new Error(
				`flex basis is not supported: ${String(result.flex)}; use flex: ${grow} to share the row or width alone to fix the column`,
			);
		result.flex = grow;
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
	if (result.verticalAlign !== undefined) verticalAlign(result.verticalAlign, 12);
	if (result.listStyleType !== undefined) listStyleType(result.listStyleType);
	if (result.listStylePosition !== undefined) listStylePosition(result.listStylePosition);
	if (result.lineHeight !== undefined) lineHeight(result.lineHeight, 12);
	const family = result.fontFamily;
	if (family !== undefined && (typeof family !== "string" || !family.trim()))
		throw new Error("fontFamily must name at least one family");
	if (result.width !== undefined) dimension(result.width, "width");
	if (result.height !== undefined) dimension(result.height, "height");
	if (result.textDecoration !== undefined) decoration(result.textDecoration);
	return result as NormalizedStyle;
}
