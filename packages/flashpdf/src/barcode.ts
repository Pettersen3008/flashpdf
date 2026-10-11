import { number, props } from "./assert.js";
import type { BarcodeProps, Element, QRCodeProps } from "./element.js";
import { qrCode, type Level } from "./qr.js";

/** Bar and space widths in modules for symbol values 0 to 106; 106 is the stop pattern. */
const PATTERNS =
	`212222 222122 222221 121223 121322 131222 122213 122312 132212 221213 221312 231212
112232 122132 122231 113222 123122 123221 223211 221132 221231 213212 223112 312131 311222 321122
321221 312212 322112 322211 212123 212321 232121 111323 131123 131321 112313 132113 132311 211313
231113 231311 112133 112331 132131 113123 113321 133121 313121 211331 231131 213113 213311 213131
311123 311321 331121 312113 312311 332111 314111 221411 431111 111224 111422 121124 121421 141122
141221 112214 112412 122114 122411 142112 142211 241211 221114 413111 241112 134111 111242 121142
121241 114212 124112 124211 411212 421112 421211 212141 214121 412121 111143 111341 131141 114113
114311 411113 411311 113141 114131 311141 411131 211412 211214 211232 2331112`.split(/\s+/);
const [START_B, START_C, CODE_B, CODE_C, STOP] = [104, 105, 100, 99, 106];

/** Code 128 symbol values: code set B for text, C for digit runs long enough to pay for the switch. */
export function code128(value: string): number[] {
	if (!/^[\x20-\x7e]+$/.test(value))
		throw new Error(
			"Barcode value must be printable ASCII, 32 to 126; code set A and FNC characters are not supported",
		);
	const symbols: number[] = [];
	let set: "B" | "C" | undefined;
	const use = (next: "B" | "C") => {
		if (set !== next)
			symbols.push(
				set === undefined ? (next === "B" ? START_B : START_C) : next === "B" ? CODE_B : CODE_C,
			);
		set = next;
	};
	for (let index = 0; index < value.length;) {
		const run = /^\d*/.exec(value.slice(index))![0].length;
		// Code C halves a digit run, and each switch costs a symbol.
		const edge = index === 0 || index + run === value.length;
		if (set === "C" ? run >= 2 : run >= (edge ? 4 : 6) || (run === value.length && run % 2 === 0)) {
			if (set !== "C" && run % 2) {
				use("B");
				symbols.push(value.charCodeAt(index++) - 32);
			}
			use("C");
			for (; /^\d\d/.test(value.slice(index, index + 2)); index += 2)
				symbols.push(Number(value.slice(index, index + 2)));
		} else {
			use("B");
			symbols.push(value.charCodeAt(index++) - 32);
		}
	}
	const checksum =
		symbols.reduce((sum, symbol, index) => sum + symbol * Math.max(index, 1), 0) % 103;
	return [...symbols, checksum, STOP];
}

const LEVEL_NAMES = ["L", "M", "Q", "H"];

function text(value: unknown, name: string): string {
	if (typeof value !== "string" || !value || /[\ud800-\udfff]/u.test(value))
		throw new Error(`${name} value must be a non-empty string`);
	return value;
}

function modules(value: unknown, fallback: number, name: string): number {
	if (value === undefined) return fallback;
	if (!Number.isInteger(value) || (value as number) < 0 || (value as number) > 100)
		throw new Error(`${name} quietZone must be a whole number of modules from 0 to 100`);
	return value as number;
}

/** The outer element's own `alt`, `style`, `className`, and `id`, as an `svg` takes them. */
function box(input: Record<string, unknown>) {
	const { alt, style, className, id } = input;
	if (alt !== undefined && typeof alt !== "string") throw new Error("alt must be a string");
	return { "aria-label": alt, style, className, id };
}

/**
 * A QR code drawn as vector paths, one rectangle per run of dark modules in a row.
 * Children draw over the symbol in a 100 by 100 box that spans it without the quiet zone.
 */
export function QRCode(input: QRCodeProps): Element {
	const p = props(input, [
		"value",
		"level",
		"quietZone",
		"alt",
		"style",
		"className",
		"id",
		"children",
	]);
	const value = text(p.value, "QRCode");
	const level = (p.level ?? "M") as Level;
	if (!LEVEL_NAMES.includes(level)) throw new Error("QRCode level must be L, M, Q, or H");
	const quiet = modules(p.quietZone, 4, "QRCode");
	const { size, dark } = qrCode(value, level);
	let d = "";
	for (let y = 0; y < size; y++)
		for (let x = 0; x < size; x++) {
			if (!dark[y * size + x]) continue;
			let end = x;
			while (end < size && dark[y * size + end]) end++;
			d += `M${x + quiet} ${y + quiet}h${end - x}v1h${x - end}z`;
			x = end;
		}
	const extent = size + 2 * quiet;
	return {
		type: "svg",
		props: {
			...box(p),
			viewBox: `0 0 ${extent} ${extent}`,
			// Two CSS pixels, 1.5 pt, per module unless a style width says otherwise.
			width: extent * 2,
			children: [
				{ type: "rect", props: { width: extent, height: extent, fill: "#fff" } },
				{ type: "path", props: { d, fill: "currentColor" } },
				{
					type: "g",
					props: {
						transform: `translate(${quiet} ${quiet}) scale(${size / 100})`,
						children: p.children,
					},
				},
			],
		},
	};
}

/** A Code 128 barcode; `moduleWidth` and `height` are points, and `text` prints the value below. */
export function Barcode(input: BarcodeProps): Element {
	const p = props(input, [
		"value",
		"moduleWidth",
		"height",
		"text",
		"quietZone",
		"alt",
		"style",
		"className",
		"id",
	]);
	const value = text(p.value, "Barcode");
	const moduleWidth = p.moduleWidth === undefined ? 1 : number(p.moduleWidth, true);
	const height = p.height === undefined ? 36 : number(p.height, true);
	if (p.text !== undefined && typeof p.text !== "boolean")
		throw new Error("Barcode text must be a boolean");
	const quiet = modules(p.quietZone, 10, "Barcode");
	let d = "";
	let x = quiet;
	for (const symbol of code128(value))
		for (const [index, width] of [...PATTERNS[symbol]!].map(Number).entries()) {
			if (index % 2 === 0) d += `M${x} 0h${width}v1h${-width}z`;
			x += width;
		}
	const extent = x + quiet;
	const { "aria-label": label, ...outer } = box(p);
	const drawing = {
		type: "svg",
		props: {
			"aria-label": label,
			viewBox: `0 0 ${extent} 1`,
			preserveAspectRatio: "none",
			width: (extent * moduleWidth) / 0.75,
			height: height / 0.75,
			children: [
				{ type: "rect", props: { width: extent, height: 1, fill: "#fff" } },
				{ type: "path", props: { d, fill: "currentColor" } },
			],
		},
	};
	if (p.text === false) return { ...drawing, props: { ...drawing.props, ...outer } };
	// A one-column flex row is the only box narrower than its container, so the text centres under the bars.
	const column = {
		type: "div",
		props: {
			style: { width: `${extent * moduleWidth}pt`, textAlign: "center", gap: 2 },
			children: [drawing, value],
		},
	};
	const row = { type: "div", props: { style: { display: "flex" }, children: column } };
	return { type: "div", props: { ...outer, children: row } };
}
