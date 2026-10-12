import { TOKEN } from "./border.js";
import { color } from "./color.js";

/** The `text-decoration` shorthand: lines, the `solid` style, and at most one colour. */
export function decoration(value: unknown): {
	underline: boolean;
	lineThrough: boolean;
	color: string | undefined;
} {
	if (typeof value !== "string") throw new Error(`invalid textDecoration: ${String(value)}`);
	const invalid = new Error(
		`invalid textDecoration: ${value} (use none, underline, or line-through)`,
	);
	const result = { underline: false, lineThrough: false, color: undefined as string | undefined };
	let none = false;
	for (const token of value.match(TOKEN) ?? []) {
		if (token === "underline") result.underline = true;
		else if (token === "line-through") result.lineThrough = true;
		else if (token === "none") none = true;
		else if (token === "overline") throw new Error("text-decoration: overline is not supported");
		else if (/^(?:dotted|dashed|double|wavy)$/.test(token))
			throw new Error(`text-decoration: ${token} is not supported; lines are solid`);
		else if (token !== "solid") {
			if (result.color !== undefined) throw invalid;
			decorationColor(token);
			result.color = token;
		}
	}
	if (none === (result.underline || result.lineThrough)) throw invalid;
	return result;
}

/** Undefined for `currentcolor`, the text colour the engine uses when no colour is sent. */
export function decorationColor(value: unknown) {
	if (typeof value === "string" && value.trim().toLowerCase() === "currentcolor") return undefined;
	return color(value);
}
