import { number, object } from "./assert.js";
import { supported, unsupported } from "./css.js";
import { flexGrow, point } from "./style/length.js";
import { normalize } from "./style/normalize.js";
import type { NormalizedStyle } from "./style/types.js";

export type { Box, FontSlots, NormalizedStyle } from "./style/types.js";
export { boxStyle, edges } from "./style/box.js";
export { color } from "./style/color.js";
export { bold, font, helvetica } from "./style/font.js";
export { point } from "./style/length.js";

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

function dimension(value: unknown, name: "width" | "height") {
	if (typeof value === "number") number(value, true);
	else if (typeof value !== "string" || !/^(?:\d+(?:\.\d*)?|\.\d+)(?:pt|px|%)$/.test(value))
		throw new Error(`invalid ${name}: ${String(value)}`);
}

export function style(value: unknown): NormalizedStyle {
	if (value === undefined) return {};
	const result = object(value);
	for (const key of Object.keys(result)) {
		if (key.startsWith("--")) continue;
		const reason = unsupported[key as keyof typeof unsupported];
		if (reason) throw new Error(`unsupported style property: ${key} (${reason})`);
		if (!supported.has(key as never)) throw new Error(`unsupported style property: ${key}`);
	}
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
	if (result.flex !== undefined) result.flex = flexGrow(result.flex);
	if (result.gap !== undefined) point(result.gap);
	if (result.fontSize !== undefined) point(result.fontSize, 12, true);
	const family = result.fontFamily;
	if (family !== undefined && (typeof family !== "string" || !family.trim()))
		throw new Error("fontFamily must name at least one family");
	if (result.width !== undefined) dimension(result.width, "width");
	if (result.height !== undefined) dimension(result.height, "height");
	if (result.textDecoration !== undefined) decoration(result.textDecoration);
	return normalize(result);
}
