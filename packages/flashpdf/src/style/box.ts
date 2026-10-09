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

export function boxStyle(s: NormalizedStyle, parent: number): Box | undefined {
	const hasBox = [
		...SIDES.flatMap((side) => [
			`margin${side}`,
			`padding${side}`,
			`border${side}Width`,
			`border${side}Color`,
		]),
		"backgroundColor",
	].some((key) => s[key as keyof NormalizedStyle] !== undefined);
	if (!hasBox) return undefined;
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
