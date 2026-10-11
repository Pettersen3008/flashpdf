import { number } from "../assert.js";

export const SIDES = ["Top", "Right", "Bottom", "Left"] as const;

export function edgeValues(value: unknown, name: "margin" | "padding"): unknown[] {
	const values = typeof value === "string" ? value.trim().split(/\s+/) : [value];
	if (values.length < 1 || values.length > 4) throw new Error(`invalid ${name}: ${value}`);
	return values.length === 1
		? [values[0], values[0], values[0], values[0]]
		: values.length === 2
			? [values[0], values[1], values[0], values[1]]
			: values.length === 3
				? [values[0], values[1], values[2], values[1]]
				: values;
}

/** Points per unit. CSS fixes 1in = 2.54cm = 25.4mm = 101.6Q = 96px = 72pt = 6pc; rem is the 12pt root size. */
const POINTS: Record<string, number> = {
	pt: 1,
	px: 0.75,
	pc: 12,
	in: 72,
	cm: 72 / 2.54,
	mm: 72 / 25.4,
	Q: 72 / 101.6,
	rem: 12,
};

export function point(value: unknown, parent = 12, positive = false): number {
	if (typeof value === "number") return number(value, positive);
	if (value === "0") return 0;
	if (typeof value !== "string") throw new Error(`invalid length: ${String(value)}`);
	const match = /^(\d+(?:\.\d*)?|\.\d+)(pt|px|pc|in|cm|mm|Q|em|rem)$/.exec(value.trim());
	if (!match) throw new Error(`invalid length: ${value}`);
	return number(Number(match[1]) * (match[2] === "em" ? parent : POINTS[match[2]!]!), positive);
}

const DIMENSION = /^(\d+(?:\.\d*)?|\.\d+)(pt|px|pc|in|cm|mm|Q|%)$/;
/** A `width` or img `height`: points (kind 1) or percent of the container width (kind 2). */
export function dimension(
	value: unknown,
	name: "width" | "height",
): { kind: 1 | 2; value: number } {
	const match = typeof value === "string" ? DIMENSION.exec(value) : null;
	const amount =
		typeof value === "number"
			? value
			: match
				? Number(match[1]) * (match[2] === "%" ? 1 : POINTS[match[2]!]!)
				: NaN;
	// The engine takes f32, so a long digit string must not round to Infinity.
	if (!(amount > 0) || !Number.isFinite(Math.fround(amount)))
		throw new Error(`invalid ${name}: ${String(value)}`);
	return { kind: match?.[2] === "%" ? 2 : 1, value: Math.fround(amount) };
}

const FLEX_KEYWORD: Record<string, number> = { none: 0, initial: 0, auto: 1 };
/** Only grow survives from the `flex` shorthand; shrink and basis have no meaning without content sizing. */
export function flexGrow(value: unknown): number {
	if (typeof value === "number") return number(value);
	if (typeof value !== "string") throw new Error(`invalid flex: ${String(value)}`);
	const first = value.trim().split(/\s+/)[0]!;
	const grow = FLEX_KEYWORD[first] ?? Number(first);
	if (Number.isNaN(grow)) throw new Error(`invalid flex: ${value}`);
	return number(grow);
}

/** Undefined is `normal`; a unitless number inherits as a factor, a length or percent as points. */
export type LineHeight = { factor: number } | { pt: number } | undefined;
export function lineHeight(value: unknown, size: number): LineHeight {
	if (value === "normal") return undefined;
	if (typeof value === "number") return { factor: number(value, true) };
	if (typeof value !== "string") throw new Error(`invalid lineHeight: ${String(value)}`);
	const match = /^(\d+(?:\.\d*)?|\.\d+)(%?)$/.exec(value.trim());
	if (!match) return { pt: point(value, size, true) };
	const amount = number(Number(match[1]), true);
	return match[2] ? { pt: number((amount * size) / 100, true) } : { factor: amount };
}
