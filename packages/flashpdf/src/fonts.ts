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

/** The core reports a glyph it cannot show as `UnsupportedCharacter('x')` (Helvetica, WinAnsi
 *  only) or `MissingGlyph('x')` (the embedded font lacks it), with the character in Rust's
 *  debug escaping; this names the character and code point and says which font to blame. */
export function glyphError(error: unknown): unknown {
	if (!(error instanceof Error)) return error;
	const match = /^(UnsupportedCharacter|MissingGlyph)\('(.+)'\)$/su.exec(error.message);
	if (!match) return error;
	const escaped = match[2]!;
	const character = escaped.startsWith("\\u{")
		? String.fromCodePoint(Number.parseInt(escaped.slice(3, -1), 16))
		: escaped.length === 2 && escaped.startsWith("\\")
			? escaped.slice(1)
			: escaped;
	const codePoint = `U+${character.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0")}`;
	return new Error(
		match[1] === "MissingGlyph"
			? `the font has no glyph for "${character}" (${codePoint})`
			: `Helvetica has no glyph for "${character}" (${codePoint}); register a font that has it`,
	);
}
