import { color } from "./color.js";
import { edgeValues, point, SIDES } from "./length.js";
import type { Box, NormalizedStyle } from "./types.js";

export function edges(
	s: NormalizedStyle,
	name: "margin" | "padding",
	parent: number,
): [number, number, number, number] {
	const shorthand = s[name];
	const values =
		shorthand === undefined ? [] : edgeValues(shorthand, name).map((value) => point(value, parent));
	const expanded =
		values.length === 1
			? [values[0], values[0], values[0], values[0]]
			: values.length === 2
				? [values[0], values[1], values[0], values[1]]
				: values.length === 3
					? [values[0], values[1], values[2], values[1]]
					: values.length === 4
						? values
						: [0, 0, 0, 0];
	return SIDES.map((side, index) =>
		s[`${name}${side}`] === undefined ? expanded[index] : point(s[`${name}${side}`], parent),
	) as [number, number, number, number];
}

export function boxStyle(s: NormalizedStyle, parent: number): Box | undefined {
	const hasBox = [
		"margin",
		"marginTop",
		"marginRight",
		"marginBottom",
		"marginLeft",
		"padding",
		"paddingTop",
		"paddingRight",
		"paddingBottom",
		"paddingLeft",
		"border",
		"borderWidth",
		"borderColor",
		"borderTop",
		"borderRight",
		"borderBottom",
		"borderLeft",
		"borderTopWidth",
		"borderRightWidth",
		"borderBottomWidth",
		"borderLeftWidth",
		"borderTopColor",
		"borderRightColor",
		"borderBottomColor",
		"borderLeftColor",
		"background",
		"backgroundColor",
	].some((key) => s[key as keyof NormalizedStyle] !== undefined);
	if (!hasBox) return undefined;
	const border = SIDES.map((side) => {
		const width = s[`border${side}Width`] ?? s.borderWidth;
		const paint = s[`border${side}Color`] ?? s.borderColor;
		return paint === "transparent" || width === undefined ? 0 : point(width, parent);
	}) as Box["border"];
	const borderColor = SIDES.map((side) => {
		const paint = s[`border${side}Color`] ?? s.borderColor;
		return paint === "transparent" || paint === undefined ? [0, 0, 0] : color(paint);
	}) as Box["borderColor"];
	const backgroundValue = s.backgroundColor ?? s.background;
	return {
		margin: edges(s, "margin", parent),
		padding: edges(s, "padding", parent),
		border,
		background:
			backgroundValue === undefined || backgroundValue === "transparent"
				? undefined
				: color(backgroundValue),
		borderColor,
	};
}
