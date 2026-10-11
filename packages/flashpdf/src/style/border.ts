import { point } from "./length.js";

const BORDER_STYLE = /^(?:none|hidden|dotted|dashed|solid|double|groove|ridge|inset|outset)$/;
// Splits on whitespace outside parentheses so `rgb(0, 0, 0)` stays one token.
const TOKEN = /[^\s()]+(?:\([^()]*\))?/g;

/** `hidden` draws like `none` outside a table with collapsed borders, which FlashPDF has not. */
export function borderStyle(value: unknown): "solid" | "none" {
	const style = typeof value === "string" ? value.trim().toLowerCase() : value;
	if (style === "solid" || style === "none") return style;
	if (style === "hidden") return "none";
	throw new Error(
		`unsupported border style: ${String(value)} (only solid, none, and hidden are drawn)`,
	);
}

/** Only the width is matched by shape; whatever is left over is handed to
 *  `color`, so an unpaintable colour reports itself instead of being ignored. */
export function parseBorder(value: unknown) {
	if (typeof value === "number") return { width: value, color: "black", style: "solid" as const };
	if (typeof value !== "string") throw new Error(`invalid border: ${String(value)}`);
	let width: unknown;
	let paint: string | undefined;
	for (const token of value.match(TOKEN) ?? []) {
		if (token === "none" || token === "hidden")
			return { width: 0, color: "black", style: "none" as const };
		if (BORDER_STYLE.test(token)) borderStyle(token);
		else if (width === undefined && /^[.\d]/.test(token)) width = token;
		else if (paint === undefined) paint = token;
		else throw new Error(`invalid border: ${value}`);
	}
	if (width === undefined) throw new Error(`invalid border: ${value}`);
	point(width);
	return { width, color: paint ?? "black", style: "solid" as const };
}
