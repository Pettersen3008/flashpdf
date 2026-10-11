import { inherited } from "../css/properties.js";
import { borderStyle, parseBorder } from "./border.js";
import { edgeValues, SIDES } from "./length.js";
import type { NormalizedStyle } from "./types.js";

/** `initial` values; a property missing here is simply absent by default. */
const INITIAL: Record<string, unknown> = {
	color: "black",
	fontSize: 12,
	fontWeight: "normal",
	fontFamily: "Helvetica",
	textAlign: "left",
	textDecoration: "none",
	lineHeight: "normal",
	backgroundColor: "transparent",
	...Object.fromEntries(
		SIDES.flatMap((side) => [
			[`margin${side}`, 0],
			[`padding${side}`, 0],
			[`border${side}Width`, 0],
			[`border${side}Color`, "currentcolor"],
		]),
	),
};
/** Absent, these follow the parent through the compiler's text state, so `inherit` leaves them absent. */
const FROM_PARENT = new Set([...inherited, "lineHeight"]);

const LOGICAL = /^(margin|padding)(Inline|Block)$/;
// Horizontal left-to-right text: inline runs from left to right, block from top to bottom.
const LOGICAL_SIDES = { Inline: ["Left", "Right"], Block: ["Top", "Bottom"] } as const;

function longhands(key: string): string[] {
	if (key === "margin" || key === "padding") return SIDES.map((side) => `${key}${side}`);
	const logical = LOGICAL.exec(key);
	if (logical)
		return LOGICAL_SIDES[logical[2] as "Inline" | "Block"].map((side) => logical[1]! + side);
	if (key === "border")
		return SIDES.flatMap((side) => [
			`border${side}Width`,
			`border${side}Color`,
			`border${side}Style`,
		]);
	if (/^border(?:Top|Right|Bottom|Left)$/.test(key))
		return [`${key}Width`, `${key}Color`, `${key}Style`];
	if (key === "borderWidth" || key === "borderColor" || key === "borderStyle")
		return SIDES.map((side) => `border${side}${key.slice(6)}`);
	return [key === "background" ? "backgroundColor" : key];
}

function keyword(item: unknown) {
	return typeof item === "string" ? item.trim().toLowerCase() : undefined;
}

/** Expands every shorthand into its longhands in cascade order, so the later declaration wins.
 *  `inherit` and `initial` resolve against `parent`, the parent's normalized style, and
 *  `currentcolor` becomes the element's own colour. */
export function normalize(
	value: Record<string, unknown>,
	parent: NormalizedStyle = {},
): NormalizedStyle {
	const own = keyword(value.color);
	const current =
		own === undefined || own === "initial"
			? "black"
			: own === "inherit" || own === "currentcolor"
				? (parent.color ?? "black")
				: (value.color as string);
	const paint = (item: unknown) =>
		typeof item === "string" ? item.replace(/\bcurrentcolor\b/gi, current) : item;
	const result: Record<string, unknown> = {};
	for (const [key, raw] of Object.entries(value)) {
		const word = key === "color" && own === "currentcolor" ? "inherit" : keyword(raw);
		const item = paint(raw);
		if (word === "inherit" || word === "initial") {
			for (const longhand of longhands(key)) {
				const next =
					word === "initial"
						? INITIAL[longhand]
						: (parent[longhand as keyof NormalizedStyle] ??
							(FROM_PARENT.has(longhand) ? undefined : INITIAL[longhand]));
				if (next === undefined) delete result[longhand];
				else result[longhand] = paint(next);
			}
		} else if (key === "margin" || key === "padding") {
			const values = edgeValues(item, key);
			for (const [index, side] of SIDES.entries()) result[`${key}${side}`] = values[index];
		} else if (LOGICAL.test(key)) {
			const values = typeof item === "string" ? item.trim().split(/\s+/) : [item];
			if (values.length > 2) throw new Error(`invalid ${key}: ${String(item)}`);
			const [first, second = first] = values;
			const [start, end] = longhands(key);
			result[start!] = first;
			result[end!] = second;
		} else if (key === "border" || /^border(?:Top|Right|Bottom|Left)$/.test(key)) {
			const border = parseBorder(item);
			for (const side of key === "border" ? SIDES : [key.slice(6)]) {
				result[`border${side}Width`] = border.width;
				result[`border${side}Color`] = border.color;
				result[`border${side}Style`] = border.style;
			}
		} else if (key === "borderWidth" || key === "borderColor") {
			for (const side of SIDES) result[`border${side}${key.slice(6)}`] = item;
		} else if (/^border(?:Top|Right|Bottom|Left)?Style$/.test(key)) {
			for (const side of key === "borderStyle" ? SIDES : [key.slice(6, -5)])
				result[`border${side}Style`] = borderStyle(item);
		} else if (key === "background") result.backgroundColor = item;
		else result[key] = item;
	}
	return result as NormalizedStyle;
}
