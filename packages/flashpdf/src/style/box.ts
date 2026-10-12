import { color } from "./color.js";
import { corner, CORNERS, point, SIDES } from "./length.js";
import type { Box, NormalizedStyle, Radius } from "./types.js";

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

/** The corner radii, or undefined when every corner is square. */
export function radiusOf(s: NormalizedStyle, parent: number): Radius | undefined {
	const values: number[] = [];
	let percent = 0;
	for (const name of CORNERS)
		for (const [amount, isPercent] of corner(s[`border${name}Radius`] ?? 0, parent)) {
			if (isPercent) percent |= 1 << values.length;
			values.push(amount);
		}
	return values.some(Boolean) ? { values, percent } : undefined;
}

const LINES = ["solid", "dashed", "dotted"];

/** Whether one stroke around the box can draw all four borders: the engine's `uniform_border`. */
function uniform(border: Box["border"], colors: Box["borderColor"], styles: Box["borderStyle"]) {
	return (
		border.every((width) => width === border[0]) &&
		(border[0] === 0 ||
			(colors.every((rgb) => rgb.every((channel, i) => channel === colors[0][i])) &&
				styles.every((line) => line === styles[0])))
	);
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
const boxes = new WeakMap<NormalizedStyle, Map<string, Box | undefined>>();

export function boxStyle(s: NormalizedStyle, parent: number, opacity = 1): Box | undefined {
	let sizes = boxes.get(s);
	if (!sizes) boxes.set(s, (sizes = new Map()));
	const key = `${parent}/${opacity}`;
	if (sizes.has(key)) return sizes.get(key);
	const box = resolveBox(s, parent, opacity);
	sizes.set(key, box);
	return box;
}

function resolveBox(s: NormalizedStyle, parent: number, opacity: number): Box | undefined {
	if (!BOX_KEYS.some((key) => s[key] !== undefined)) return undefined;
	const lines = SIDES.map((side) => s[`border${side}Style`] ?? "solid");
	const border = SIDES.map((side, index) => {
		const width = s[`border${side}Width`];
		const paint = s[`border${side}Color`];
		return paint === "transparent" || width === undefined || lines[index] === "none"
			? 0
			: point(width, parent);
	}) as Box["border"];
	const borderColor = SIDES.map((side) => {
		const paint = s[`border${side}Color`];
		return paint === "transparent" || paint === undefined ? [0, 0, 0] : color(paint);
	}) as Box["borderColor"];
	const borderStyle = lines.map((line) => Math.max(0, LINES.indexOf(line))) as Box["borderStyle"];
	const radius = radiusOf(s, parent);
	if (radius && !uniform(border, borderColor, borderStyle))
		throw new Error(
			"border-radius needs one width, colour, and style on all four borders, or none, " +
				"because a curved border is drawn as one stroke",
		);
	return {
		margin: edges(s, "margin", parent),
		padding: edges(s, "padding", parent),
		border,
		background:
			s.backgroundColor === undefined || s.backgroundColor === "transparent"
				? undefined
				: color(s.backgroundColor),
		borderColor,
		borderStyle,
		radius,
		opacity,
	};
}
