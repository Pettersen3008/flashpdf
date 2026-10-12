import { number } from "../assert.js";

export const SIDES = ["Top", "Right", "Bottom", "Left"] as const;
export const CORNERS = ["TopLeft", "TopRight", "BottomRight", "BottomLeft"] as const;

export function edgeValues(value: unknown, name: string): unknown[] {
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

const NUMBER = String.raw`(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?`;
const LENGTH = new RegExp(`^(${NUMBER})(pt|px|pc|in|cm|mm|Q|em|rem)$`);
const PERCENT = new RegExp(`^(${NUMBER})%$`);
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
	const match = LENGTH.exec(value.trim());
	if (!match) throw new Error(`invalid length: ${value}`);
	return number(Number(match[1]) * (match[2] === "em" ? parent : POINTS[match[2]!]!), positive);
}

/** One radius: points, or a percent of the box side, which only the engine can resolve. */
type Radius = [amount: number, percent: boolean];
/** A corner's radius, horizontal then vertical; one value serves both. */
export function corner(value: unknown, parent: number): [Radius, Radius] {
	const tokens = typeof value === "string" ? value.trim().split(/\s+/) : [value];
	if (tokens.length > 2) throw new Error(`invalid border radius: ${String(value)}`);
	const radius = (token: unknown): Radius => {
		const percent = typeof token === "string" ? PERCENT.exec(token) : null;
		return percent ? [number(Number(percent[1])), true] : [point(token, parent), false];
	};
	return [radius(tokens[0]), radius(tokens[1] ?? tokens[0])];
}

/** A number or percent, clamped to 0 through 1 as CSS does. */
export function opacity(value: unknown): number {
	const percent =
		typeof value === "string" ? /^(\d+(?:\.\d*)?|\.\d+)(%?)$/.exec(value.trim()) : null;
	const amount =
		typeof value === "number" ? value : percent ? Number(percent[1]) / (percent[2] ? 100 : 1) : NaN;
	if (Number.isNaN(amount)) throw new Error(`invalid opacity: ${String(value)}`);
	return Math.min(1, Math.max(0, amount));
}

/** `letter-spacing` or `word-spacing` in points, either sign, bounded by the largest page side. */
export function spacing(value: unknown, size: number, name: string): number {
	if (value === "normal") return 0;
	const negative =
		typeof value === "number" ? value < 0 : typeof value === "string" && /^\s*-/.test(value);
	const magnitude =
		typeof value === "number" ? Math.abs(value) : String(value).trim().replace(/^-/, "");
	let amount: number;
	try {
		amount = point(magnitude, size) * (negative ? -1 : 1);
	} catch {
		throw new Error(`invalid ${name}: ${String(value)} (use normal or a length)`);
	}
	if (Math.abs(amount) > 14_400) throw new Error(`invalid ${name}: ${String(value)}`);
	return amount;
}

const DIMENSION = /^(\d+(?:\.\d*)?|\.\d+)(pt|px|pc|in|cm|mm|Q|rem|%)$/;

/** A `width` or img `height`: points (kind 1) or percent of the container width (kind 2). */
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
					? Number(match[1]) * (match[2] === "%" ? 1 : POINTS[match[2]!]!)
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
