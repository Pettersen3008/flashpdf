import { inherited } from "../css/properties.js";
import { parseBorder } from "./border.js";
import { CORNERS, edgeValues, SIDES } from "./length.js";
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
	opacity: 1,
	...Object.fromEntries(
		SIDES.flatMap((side) => [
			[`margin${side}`, 0],
			[`padding${side}`, 0],
			[`border${side}Width`, 0],
			[`border${side}Color`, "currentcolor"],
			// A border without a style is solid here, where CSS draws none.
			[`border${side}Style`, "solid"],
		]),
	),
	...Object.fromEntries(CORNERS.map((corner) => [`border${corner}Radius`, 0])),
};
/** Absent, these follow the parent through the compiler's text state, so `inherit` leaves them absent. */
const FROM_PARENT = new Set([...inherited, "lineHeight"]);

function longhands(key: string): string[] {
	if (key === "margin" || key === "padding") return SIDES.map((side) => `${key}${side}`);
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
	if (key === "borderRadius") return CORNERS.map((corner) => `border${corner}Radius`);
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
		} else if (key === "border" || /^border(?:Top|Right|Bottom|Left)$/.test(key)) {
			const border = parseBorder(item);
			for (const side of key === "border" ? SIDES : [key.slice(6)]) {
				result[`border${side}Width`] = border.width;
				result[`border${side}Color`] = border.color;
				result[`border${side}Style`] = border.style;
			}
		} else if (key === "borderWidth" || key === "borderColor") {
			for (const side of SIDES) result[`border${side}${key.slice(6)}`] = item;
		} else if (key === "borderStyle") {
			const values = edgeValues(item, "border-style");
			for (const [index, side] of SIDES.entries()) result[`border${side}Style`] = values[index];
		} else if (key === "borderRadius") {
			// Horizontal radii, then after a slash the vertical ones, each as margin gives its four sides.
			const parts = typeof item === "string" ? item.split("/") : [item];
			if (parts.length > 2) throw new Error(`invalid border-radius: ${String(item)}`);
			const across = edgeValues(parts[0], "border-radius");
			const down = parts[1] === undefined ? across : edgeValues(parts[1], "border-radius");
			for (const [index, corner] of CORNERS.entries())
				result[`border${corner}Radius`] =
					parts[1] === undefined
						? across[index]
						: `${String(across[index])} ${String(down[index])}`;
		} else if (key === "background") result.backgroundColor = item;
		else result[key] = item;
	}
	return result as NormalizedStyle;
}
