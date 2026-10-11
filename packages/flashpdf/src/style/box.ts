import { color } from "./color.js";
import { point, SIDES } from "./length.js";
import type { Box, NormalizedStyle } from "./types.js";

export function edges(
	s: NormalizedStyle,
	name: "margin" | "padding",
	parent: number,
): [number, number, number, number] {
	return SIDES.map((side) => point(s[`${name}${side}`] ?? 0, parent)) as [
		number,
		number,
		number,
		number,
	];
}

const BOX_KEYS = [
	...SIDES.flatMap((side) => [
		`margin${side}`,
		`padding${side}`,
		`border${side}Width`,
		`border${side}Color`,
	]),
	"backgroundColor",
] as (keyof NormalizedStyle)[];

/** Per computed style, then parent font size; rows share computed styles, so most calls repeat. */
const boxes = new WeakMap<NormalizedStyle, Map<number, Box | undefined>>();

export function boxStyle(s: NormalizedStyle, parent: number): Box | undefined {
	let sizes = boxes.get(s);
	if (!sizes) boxes.set(s, (sizes = new Map()));
	if (sizes.has(parent)) return sizes.get(parent);
	const box = resolveBox(s, parent);
	sizes.set(parent, box);
	return box;
}

function resolveBox(s: NormalizedStyle, parent: number): Box | undefined {
	if (!BOX_KEYS.some((key) => s[key] !== undefined)) return undefined;
	const border = SIDES.map((side) => {
		const width = s[`border${side}Width`];
		const paint = s[`border${side}Color`];
		return paint === "transparent" || width === undefined ? 0 : point(width, parent);
	}) as Box["border"];
	const borderColor = SIDES.map((side) => {
		const paint = s[`border${side}Color`];
		return paint === "transparent" || paint === undefined ? [0, 0, 0] : color(paint);
	}) as Box["borderColor"];
	return {
		margin: edges(s, "margin", parent),
		padding: edges(s, "padding", parent),
		border,
		background:
			s.backgroundColor === undefined || s.backgroundColor === "transparent"
				? undefined
				: color(s.backgroundColor),
		borderColor,
	};
}
