import type { ListStyleType } from "../element.js";

const TYPES = new Set<string>([
	"disc",
	"circle",
	"square",
	"decimal",
	"decimal-leading-zero",
	"lower-alpha",
	"lower-latin",
	"upper-alpha",
	"upper-latin",
	"lower-roman",
	"upper-roman",
	"none",
]);

export function listStyleType(value: unknown): ListStyleType {
	const type = typeof value === "string" ? value.trim().toLowerCase() : value;
	if (typeof type !== "string" || !TYPES.has(type))
		throw new Error(
			`invalid listStyleType: ${String(value)} (use disc, circle, square, decimal, decimal-leading-zero, lower-alpha, lower-latin, upper-alpha, upper-latin, lower-roman, upper-roman, or none)`,
		);
	return type as ListStyleType;
}

export function listStylePosition(value: unknown): "outside" | "inside" {
	const position = typeof value === "string" ? value.trim().toLowerCase() : value;
	if (position !== "outside" && position !== "inside")
		throw new Error(`invalid listStylePosition: ${String(value)} (use outside or inside)`);
	return position;
}

/** The `list-style` shorthand. A part left out resets to its initial value, `disc` and `outside`. */
export function parseListStyle(value: unknown): {
	type: ListStyleType;
	position: "outside" | "inside";
} {
	if (typeof value !== "string") throw new Error(`invalid listStyle: ${String(value)}`);
	let type: ListStyleType | undefined;
	let position: "outside" | "inside" | undefined;
	let nones = 0;
	for (const token of value.trim().split(/\s+/)) {
		const word = token.toLowerCase();
		if (word === "inside" || word === "outside") {
			if (position) throw new Error(`invalid listStyle: ${value}`);
			position = word;
		} else if (word === "none") nones++;
		else if (word.includes("("))
			throw new Error("list-style-image is not supported; markers are text or drawn shapes");
		else if (type) throw new Error(`invalid listStyle: ${value}`);
		else type = listStyleType(word);
	}
	// A lone `none` is the type, and a second one is the image, which is none here.
	if (nones > (type ? 1 : 2)) throw new Error(`invalid listStyle: ${value}`);
	return { type: type ?? (nones ? "none" : "disc"), position: position ?? "outside" };
}

const ROMAN: [number, string][] = [
	[1000, "m"],
	[900, "cm"],
	[500, "d"],
	[400, "cd"],
	[100, "c"],
	[90, "xc"],
	[50, "l"],
	[40, "xl"],
	[10, "x"],
	[9, "ix"],
	[5, "v"],
	[4, "iv"],
	[1, "i"],
];

/** A bullet is a shape the engine draws; a counter is text in the item's font. A counter
 *  outside the range of its system, such as zero in `lower-alpha`, falls back to decimal. */
export const SHAPES: Partial<Record<ListStyleType, string>> = {
	disc: "•",
	circle: "◦",
	square: "▪",
};

export function counter(type: ListStyleType, n: number): string {
	const upper = type.startsWith("upper");
	let text: string | undefined;
	if (type === "decimal-leading-zero")
		text = `${n < 0 ? "-" : ""}${String(Math.abs(n)).padStart(2, "0")}`;
	else if (type.endsWith("alpha") || type.endsWith("latin")) {
		if (n >= 1) {
			text = "";
			for (let rest = n; rest > 0; rest = Math.floor((rest - 1) / 26))
				text = String.fromCharCode(97 + ((rest - 1) % 26)) + text;
		}
	} else if (type.endsWith("roman")) {
		if (n >= 1 && n <= 3999) {
			text = "";
			let rest = n;
			for (const [value, numeral] of ROMAN) for (; rest >= value; rest -= value) text += numeral;
		}
	}
	text ??= String(n);
	return upper ? text.toUpperCase() : text;
}
