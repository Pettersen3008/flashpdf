import type { PdfRenderer } from "../wasm/flashpdf_wasm.js";
import { props } from "./assert.js";
import { type FontSlots, helvetica } from "./style.js";

export function registerFonts(renderer: PdfRenderer, value: unknown): Map<string, FontSlots> {
	const result = new Map<string, FontSlots>([["Helvetica", helvetica]]);
	if (value === undefined) return result;
	if (!Array.isArray(value)) throw new Error("invalid fonts");
	for (const entry of value) {
		const font = props(entry, ["family", "regular", "bold"]);
		if (typeof font.family !== "string" || !font.family.trim() || font.family.includes(","))
			throw new Error("font family must name one registered family");
		if (result.has(font.family.trim())) throw new Error(`duplicate font family: ${font.family}`);
		if (!(font.regular instanceof Uint8Array) || !font.regular.byteLength)
			throw new Error("font regular must be a non-empty Uint8Array");
		const regular = renderer.add_font(font.regular);
		let boldFace: number | undefined;
		if (font.bold !== undefined) {
			if (!(font.bold instanceof Uint8Array) || !font.bold.byteLength)
				throw new Error("font bold must be a non-empty Uint8Array");
			boldFace = renderer.add_font(font.bold);
		}
		result.set(font.family.trim(), { regular, bold: boldFace });
	}
	return result;
}
