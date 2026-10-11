import { parseBorder } from "./border.js";
import { decoration } from "./decoration.js";
import { edgeValues, SIDES } from "./length.js";
import { parseListStyle } from "./list.js";
import type { NormalizedStyle } from "./types.js";

/** Expands every shorthand into its longhands in cascade order, so the later declaration wins. */
export function normalize(value: Record<string, unknown>): NormalizedStyle {
	const result: Record<string, unknown> = {};
	for (const [key, item] of Object.entries(value)) {
		if (key === "margin" || key === "padding") {
			const values = edgeValues(item, key);
			for (const [index, side] of SIDES.entries()) result[`${key}${side}`] = values[index];
		} else if (key === "border" || /^border(?:Top|Right|Bottom|Left)$/.test(key)) {
			const border = parseBorder(item);
			for (const side of key === "border" ? SIDES : [key.slice(6)]) {
				result[`border${side}Width`] = border.width;
				result[`border${side}Color`] = border.color;
			}
		} else if (key === "borderWidth" || key === "borderColor") {
			for (const side of SIDES) result[`border${side}${key.slice(6)}`] = item;
		} else if (key === "textDecoration") {
			// The shorthand resets the colour, so a later `text-decoration: underline` drops an earlier one.
			result.textDecoration = item;
			result.textDecorationColor = decoration(item).color ?? "currentcolor";
		} else if (key === "listStyle") {
			// The shorthand resets both longhands, so a later `list-style: inside` brings back `disc`.
			const parsed = parseListStyle(item);
			result.listStyleType = parsed.type;
			result.listStylePosition = parsed.position;
		} else if (key === "background") result.backgroundColor = item;
		else result[key] = item;
	}
	return result as NormalizedStyle;
}
