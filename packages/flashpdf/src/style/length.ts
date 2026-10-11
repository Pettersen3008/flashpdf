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
const LENGTH = new RegExp(`^(${NUMBER})(pt|px|em|rem)$`);
const PERCENT = new RegExp(`^(${NUMBER})%$`);

export function point(value: unknown, parent = 12, positive = false): number {
	if (typeof value === "number") return number(value, positive);
	if (value === "0") return 0;
	if (typeof value !== "string") throw new Error(`invalid length: ${String(value)}`);
	const match = LENGTH.exec(value.trim());
	if (!match) throw new Error(`invalid length: ${value}`);
	const scale = match[2] === "px" ? 0.75 : match[2] === "em" ? parent : match[2] === "rem" ? 12 : 1;
	return number(Number(match[1]) * scale, positive);
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

const DIMENSION = /^(\d+(?:\.\d*)?|\.\d+)(pt|px|rem|%)$/;

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
				? Number(match[1]) * (match[2] === "px" ? 0.75 : match[2] === "rem" ? 12 : 1)
				: NaN;
	// The engine takes f32, so a long digit string must not round to Infinity.
	if (!(amount > 0) || !Number.isFinite(Math.fround(amount)))
		throw new Error(`invalid ${name}: ${String(value)}`);
	return { kind: match?.[2] === "%" ? 2 : 1, value: Math.fround(amount) };
}

const FLEX_KEYWORD = new Map([
	["none", 0],
	["initial", 0],
	["auto", 1],
]);
/** Only grow survives from the `flex` shorthand; shrink has no meaning without content sizing.
 *  The basis is reported so a caller can reject one that would be dropped. */
export function flexGrow(value: unknown): { grow: number; basis: "zero" | "auto" | "length" } {
	if (typeof value === "number") return { grow: number(value), basis: "zero" };
	if (typeof value !== "string") throw new Error(`invalid flex: ${String(value)}`);
	const [first, ...rest] = value.trim().split(/\s+/);
	const keyword = FLEX_KEYWORD.get(first!);
	if (keyword !== undefined) return { grow: keyword, basis: "auto" };
	const grow = Number(first);
	if (Number.isNaN(grow)) throw new Error(`invalid flex: ${value}`);
	// After grow and an optional unitless shrink, the first other token is the basis.
	const basis = rest.find((token) => !/^(?:\d+(?:\.\d*)?|\.\d+)$/.test(token));
	return {
		grow: number(grow),
		basis:
			basis === undefined || /^(?:0+\.?0*|\.0+)(?:%|[a-z]+)?$/.test(basis)
				? "zero"
				: basis === "auto"
					? "auto"
					: "length",
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
