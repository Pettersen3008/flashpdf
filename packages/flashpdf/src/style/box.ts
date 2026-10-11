import { color } from "./color.js";
import { dimension, point, SIDES } from "./length.js";
import type { Box, NormalizedStyle, Size } from "./types.js";

export function edges(
	s: NormalizedStyle,
	name: "margin" | "padding",
	parent: number,
): [number, number, number, number] {
	// An `auto` margin is zero here; `boxSize` flags it.
	return SIDES.map((side) => {
		const value = s[`${name}${side}`] ?? 0;
		return name === "margin" && value === "auto" ? 0 : point(value, parent);
	}) as [number, number, number, number];
}

/** The box's sizes and auto margins, or undefined when it has none. `height` is the parent's
 *  definite content height, which percent heights need; `width` is false where a table or
 *  column-model flex row owns the width. */
export function boxSize(
	s: NormalizedStyle,
	height: number | undefined,
	width = true,
): Size | undefined {
	const autoMargin = SIDES.reduce(
		(bits, side, index) => (s[`margin${side}`] === "auto" ? bits | (1 << index) : bits),
		0,
	);
	const horizontal = (key: "width" | "minWidth" | "maxWidth") =>
		s[key] === undefined ? undefined : dimension(s[key], key, true);
	const vertical = (key: "height" | "minHeight" | "maxHeight") => {
		if (s[key] === undefined) return undefined;
		const { kind, value } = dimension(s[key], key, true);
		if (kind === 1) return value;
		if (height === undefined)
			throw new Error(`a percent ${key} needs a parent with a fixed height; use pt or px`);
		return Math.fround((height * value) / 100);
	};
	const size: Size = {
		borderBox: s.boxSizing === "border-box",
		autoMargin,
		width: width ? horizontal("width") : undefined,
		minWidth: horizontal("minWidth"),
		maxWidth: horizontal("maxWidth"),
		height: vertical("height"),
		minHeight: vertical("minHeight"),
		maxHeight: vertical("maxHeight"),
	};
	return autoMargin ||
		[size.width, size.minWidth, size.maxWidth, size.height, size.minHeight, size.maxHeight].some(
			(value) => value !== undefined,
		)
		? size
		: undefined;
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
