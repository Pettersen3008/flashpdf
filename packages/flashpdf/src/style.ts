import { object } from "./assert.js";
import { supported, unsupported } from "./css/properties.js";
import { bold } from "./style/font.js";
import { dimension, flexBasis, lineHeight, point } from "./style/length.js";
import { normalize } from "./style/normalize.js";
import type { NormalizedStyle } from "./style/types.js";

export type { Box, FontSlots, NormalizedStyle, Size } from "./style/types.js";
export { boxSize, boxStyle, edges } from "./style/box.js";
export { color } from "./style/color.js";
export { bold, font, helvetica } from "./style/font.js";
export { dimension, flexBasis, lineHeight, point, type LineHeight } from "./style/length.js";

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

export function decoration(value: unknown): { underline: boolean; lineThrough: boolean } {
	if (typeof value !== "string") throw new Error(`invalid textDecoration: ${String(value)}`);
	const tokens = value.trim().split(/\s+/);
	const result = { underline: false, lineThrough: false };
	for (const token of tokens) {
		if (token === "underline") result.underline = true;
		else if (token === "line-through") result.lineThrough = true;
		else if (token === "overline") throw new Error("text-decoration: overline is not supported");
		else if (token !== "none" || tokens.length > 1)
			throw new Error(`invalid textDecoration: ${value} (use none, underline, or line-through)`);
	}
	return result;
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
		result.textAlign !== "right"
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
	if (result.lineHeight !== undefined) lineHeight(result.lineHeight, 12);
	const family = result.fontFamily;
	if (family !== undefined && (typeof family !== "string" || !family.trim()))
		throw new Error("fontFamily must name at least one family");
	if (result.textDecoration !== undefined) decoration(result.textDecoration);
	return result as NormalizedStyle;
}
