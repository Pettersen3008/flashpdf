import { parseBorder } from "./border.js";
import { edgeValues, SIDES } from "./length.js";
import type { NormalizedStyle } from "./types.js";

export function normalize(value: Record<string, unknown>): NormalizedStyle {
	const result: Record<string, unknown> = {};
	for (const [key, item] of Object.entries(value)) {
		if (key === "margin" || key === "padding") {
			const values = edgeValues(item, key);
			for (const [index, side] of SIDES.entries()) result[`${key}${side}`] = values[index];
			continue;
		}
		if (key === "border") {
			const border = parseBorder(item);
			result.borderWidth = border.width;
			result.borderColor = border.color;
			continue;
		}
		if (/^border(?:Top|Right|Bottom|Left)$/.test(key)) {
			const border = parseBorder(item);
			result[`${key}Width`] = border.width;
			result[`${key}Color`] = border.color;
			continue;
		}
		result[key] = item;
	}
	return result as NormalizedStyle;
}
