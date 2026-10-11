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

export function point(value: unknown, parent = 12, positive = false): number {
	if (typeof value === "number") return number(value, positive);
	if (value === "0") return 0;
	if (typeof value !== "string") throw new Error(`invalid length: ${String(value)}`);
	const match = /^(\d+(?:\.\d*)?|\.\d+)(pt|px|em|rem)$/.exec(value.trim());
	if (!match) throw new Error(`invalid length: ${value}`);
	const scale = match[2] === "px" ? 0.75 : match[2] === "em" ? parent : match[2] === "rem" ? 12 : 1;
	return number(Number(match[1]) * scale, positive);
}

const DIMENSION = /^(\d+(?:\.\d*)?|\.\d+)(pt|px|rem|%)$/;
/** A width or height: points (kind 1) or percent of the container (kind 2). Only box sizes take
 *  `zero`, which is always points. */
export function dimension(
	value: unknown,
	name: string,
	zero = false,
): { kind: 1 | 2; value: number } {
	const match = typeof value === "string" ? DIMENSION.exec(value) : null;
	const amount =
		typeof value === "number"
			? value
			: value === "0"
				? 0
				: match
					? Number(match[1]) * (match[2] === "px" ? 0.75 : match[2] === "rem" ? 12 : 1)
					: NaN;
	// The engine takes f32, so a long digit string must not round to Infinity.
	if (!(amount > 0 || (zero && amount === 0)) || !Number.isFinite(Math.fround(amount)))
		throw new Error(`invalid ${name}: ${String(value)}`);
	return { kind: match?.[2] === "%" && amount > 0 ? 2 : 1, value: Math.fround(amount) };
}

/** `auto`, or a zero, length, or percent basis. */
export function flexBasis(value: unknown) {
	if (value === "auto") return undefined;
	if (value === "content") throw new Error("flex-basis: content is not supported; use auto");
	return dimension(value, "flex-basis", true);
}

const FACTOR = /^(?:\d+(?:\.\d*)?|\.\d+)$/;
/** The `flex` shorthand's longhands: `none`, `auto`, or grow and shrink factors with a basis
 *  before or after them. Factors alone have a zero basis; a basis alone grows and shrinks by 1. */
export function flexShorthand(value: unknown): {
	flexGrow: number;
	flexShrink: number;
	flexBasis: unknown;
} {
	if (typeof value === "number") return { flexGrow: number(value), flexShrink: 1, flexBasis: 0 };
	const text = typeof value === "string" ? value.trim().toLowerCase() : "";
	if (!text) throw new Error(`invalid flex: ${String(value)}`);
	if (text === "none") return { flexGrow: 0, flexShrink: 0, flexBasis: "auto" };
	if (text === "auto") return { flexGrow: 1, flexShrink: 1, flexBasis: "auto" };
	const tokens = text.split(/\s+/);
	let basis = FACTOR.test(tokens[0]!) ? undefined : tokens.shift();
	let [grow, shrink, last, ...extra] = tokens;
	// A second token that is no number, or any third token, is the basis.
	if (shrink !== undefined && !FACTOR.test(shrink) && last === undefined)
		[shrink, last] = [undefined, shrink];
	if (last !== undefined) {
		if (basis !== undefined) extra.push(last);
		basis = last;
	}
	if (extra.length || [grow, shrink].some((token) => token !== undefined && !FACTOR.test(token)))
		throw new Error(`invalid flex: ${String(value)}`);
	flexBasis(basis ?? 0);
	return {
		flexGrow: number(Number(grow ?? 1)),
		flexShrink: number(Number(shrink ?? 1)),
		flexBasis: basis ?? 0,
	};
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
