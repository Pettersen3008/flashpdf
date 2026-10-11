import { point } from "./length.js";

const BORDER_STYLE = /^(?:none|hidden|dotted|dashed|solid|double|groove|ridge|inset|outset)$/;
// Splits on whitespace outside parentheses so `rgb(0, 0, 0)` stays one token.
const TOKEN = /[^\s()]+(?:\([^()]*\))?/g;

export type Line = "none" | "solid" | "dashed" | "dotted";

/** `hidden` draws like `none`. A border with no style at all is solid, unlike CSS, where it is `none`. */
export function line(value: unknown): Line {
	const word = typeof value === "string" ? value.trim().toLowerCase() : value;
	if (word === "none" || word === "hidden") return "none";
	if (word === "solid" || word === "dashed" || word === "dotted") return word;
	if (typeof word === "string" && BORDER_STYLE.test(word))
		throw new Error(
			`unsupported border style: ${word} (only none, solid, dashed, and dotted are drawn)`,
		);
	throw new Error(`invalid border style: ${String(value)}`);
}

/** Only the width is matched by shape; whatever is left over is handed to
 *  `color`, so an unpaintable colour reports itself instead of being ignored. */
export function parseBorder(value: unknown) {
	if (typeof value === "number") return { width: value, color: "black", style: "solid" as Line };
	if (typeof value !== "string") throw new Error(`invalid border: ${String(value)}`);
	let width: unknown;
	let paint: string | undefined;
	let style: Line = "solid";
	for (const token of value.match(TOKEN) ?? []) {
		if (token === "none" || token === "hidden") return { width: 0, color: "black", style };
		if (BORDER_STYLE.test(token)) style = line(token);
		else if (width === undefined && /^[.\d]/.test(token)) width = token;
		else if (paint === undefined) paint = token;
		else throw new Error(`invalid border: ${value}`);
	}
	if (width === undefined) throw new Error(`invalid border: ${value}`);
	point(width);
	return { width, color: paint ?? "black", style };
}
