import { object, number } from "./assert.js";
import { supported, unsupported } from "./css.js";
import type { Length, StyleInput } from "./element.js";

export type NormalizedStyle = StyleInput & {
	borderTopWidth?: Length;
	borderRightWidth?: Length;
	borderBottomWidth?: Length;
	borderLeftWidth?: Length;
	borderTopColor?: string;
	borderRightColor?: string;
	borderBottomColor?: string;
	borderLeftColor?: string;
};

const SIDES = ["Top", "Right", "Bottom", "Left"] as const;

export function style(value: unknown, tag = ""): NormalizedStyle {
	if (value === undefined) return {};
	const where = tag && ` on <${tag}>`;
	const result = object(value);
	for (const key of Object.keys(result)) {
		if (key.startsWith("--")) continue;
		const reason = unsupported[key as keyof typeof unsupported];
		if (reason) throw new Error(`unsupported style property: ${key} (${reason})${where}`);
		if (!supported.has(key as never)) throw new Error(`unsupported style property: ${key}${where}`);
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
	if (result.flex !== undefined) number(result.flex, true);
	if (result.gap !== undefined) point(result.gap);
	if (result.fontSize !== undefined) point(result.fontSize, 12, true);
	const family = result.fontFamily;
	if (
		family !== undefined &&
		(typeof family !== "string" || !family.trim() || family.includes(","))
	)
		throw new Error("fontFamily must name one registered family");
	if (result.width !== undefined) {
		if (typeof result.width === "number") number(result.width, true);
		else if (
			typeof result.width !== "string" ||
			!/^(?:\d+(?:\.\d*)?|\.\d+)(?:pt|px|%)$/.test(result.width)
		)
			throw new Error("invalid width");
	}
	return normalize(result);
}

function edgeValues(value: unknown, name: "margin" | "padding"): unknown[] {
	const values = typeof value === "string" ? value.trim().split(/\s+/) : [value];
	if (values.length < 1 || values.length > 4) throw new Error(`invalid ${name}`);
	return values.length === 1
		? [values[0], values[0], values[0], values[0]]
		: values.length === 2
			? [values[0], values[1], values[0], values[1]]
			: values.length === 3
				? [values[0], values[1], values[2], values[1]]
				: values;
}

function normalize(value: Record<string, unknown>): NormalizedStyle {
	const result: Record<string, unknown> = {};
	for (const [key, item] of Object.entries(value)) {
		if (key === "margin" || key === "padding") {
			for (const [index, side] of SIDES.entries())
				result[`${key}${side}`] = edgeValues(item, key)[index];
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

const BORDER_STYLE = /^(?:none|hidden|dotted|dashed|solid|double|groove|ridge|inset|outset)$/;

/** Only the width is matched by shape; whatever is left over is handed to
 *  `color`, so an unpaintable colour reports itself instead of being ignored. */
function parseBorder(value: unknown) {
	if (typeof value === "number") return { width: value, color: "black" };
	if (typeof value !== "string") throw new Error("invalid border");
	let width: unknown;
	let paint: string | undefined;
	for (const token of value.trim().split(/\s+/)) {
		if (token === "none" || token === "hidden") return { width: 0, color: "black" };
		if (BORDER_STYLE.test(token)) {
			if (token !== "solid") throw new Error(`unsupported border style: ${token}`);
		} else if (width === undefined && /^[.\d]/.test(token)) width = token;
		else if (paint === undefined) paint = token;
		else throw new Error("invalid border");
	}
	if (width === undefined) throw new Error("invalid border");
	point(width);
	return { width, color: paint ?? "black" };
}

export function point(value: unknown, parent = 12, positive = false): number {
	if (typeof value === "number") return number(value, positive);
	if (value === "0") return 0;
	if (typeof value !== "string") throw new Error("invalid length");
	const match = /^(\d+(?:\.\d*)?|\.\d+)(pt|px|em|rem)$/.exec(value.trim());
	if (!match) throw new Error("invalid length");
	const scale = match[2] === "px" ? 0.75 : match[2] === "em" ? parent : match[2] === "rem" ? 12 : 1;
	return number(Number(match[1]) * scale, positive);
}

export type Box = {
	margin: [number, number, number, number];
	padding: [number, number, number, number];
	border: [number, number, number, number];
	background?: [number, number, number] | undefined;
	borderColor: [
		[number, number, number],
		[number, number, number],
		[number, number, number],
		[number, number, number],
	];
};

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

export function color(value: unknown): [number, number, number] {
	if (typeof value !== "string") throw new Error("invalid color");
	const hex = value.trim().toLowerCase();
	const named: Record<string, string> = {
		black: "#000000",
		white: "#ffffff",
		red: "#ff0000",
		green: "#008000",
		blue: "#0000ff",
	};
	const source = named[hex] ?? hex;
	const match = /^#([\da-f]{3}|[\da-f]{6})$/.exec(source);
	if (!match)
		throw new Error(
			hex === "transparent"
				? "transparent applies only to background and border color"
				: `unsupported color: ${hex}`,
		);
	const group = match[1]!;
	const value6 = group.length === 3 ? [...group].map((part) => part + part).join("") : group;
	return [
		Number.parseInt(value6.slice(0, 2), 16),
		Number.parseInt(value6.slice(2, 4), 16),
		Number.parseInt(value6.slice(4, 6), 16),
	];
}

export function bold(value: unknown): boolean {
	return (
		value === "bold" ||
		(typeof value === "number" && value >= 600) ||
		(typeof value === "string" && Number(value) >= 600)
	);
}

export type FontSlots = { regular: number; bold?: number | undefined };
export const helvetica: FontSlots = { regular: 0, bold: 1 };

export function font(
	s: NormalizedStyle,
	inherited: FontSlots,
	inheritedBold: boolean,
	fonts: ReadonlyMap<string, FontSlots>,
) {
	const requested = s.fontFamily;
	const family =
		requested === undefined
			? inherited
			: typeof requested === "string"
				? fonts.get(requested.trim())
				: undefined;
	if (!family) throw new Error(`unregistered font family: ${requested}`);
	const weight = s.fontWeight === undefined ? inheritedBold : bold(s.fontWeight);
	if (weight && family.bold === undefined)
		throw new Error(`font family has no bold face: ${s.fontFamily ?? "inherited"}`);
	return { family, weight };
}

export function nativeColumn(value: unknown): { kind: number; value: number } {
	if (typeof value !== "object" || value === null || Array.isArray(value))
		return { kind: 1, value: 1 };
	const node = object(value);
	const p = object(node.props);
	const s = style(p.style);
	if (typeof s.flex === "number") return { kind: 1, value: number(s.flex, true) };
	if (typeof s.width === "number") return { kind: 0, value: number(s.width, true) };
	if (typeof s.width === "string") {
		const value = number(Number.parseFloat(s.width), true);
		return s.width.endsWith("pt")
			? { kind: 0, value }
			: s.width.endsWith("px")
				? { kind: 0, value: value * 0.75 }
				: { kind: 2, value };
	}
	return { kind: 1, value: 1 };
}
