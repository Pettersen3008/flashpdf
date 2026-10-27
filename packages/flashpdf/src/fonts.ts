import { props } from "./assert.js";
import { asError, type Engine } from "./binary.js";

/** Engine font slots per face of a family; 0 to 3 are the built-in Helvetica faces. */
export type FontSlots = {
	regular: number;
	bold?: number | undefined;
	italic?: number | undefined;
	boldItalic?: number | undefined;
};
const helvetica: FontSlots = { regular: 0, bold: 1, italic: 2, boldItalic: 3 };

/** The core's cap, checked first because `add_font` copies the bytes into WASM memory. */
const MAX_FONT_BYTES = 64 * 1024 * 1024;

export function registerFonts(
	renderer: Pick<Engine, "add_font">,
	value: unknown,
): Map<string, FontSlots> {
	const result = new Map<string, FontSlots>([["Helvetica", helvetica]]);
	if (value === undefined) return result;
	if (!Array.isArray(value)) throw new Error("invalid fonts");
	let total = 0;
	const addFont = (bytes: Uint8Array, family: string): number => {
		total += bytes.byteLength;
		if (total > MAX_FONT_BYTES)
			throw new Error(`font family "${family}": fonts exceed 64 MiB per render`);
		try {
			return renderer.add_font(bytes);
		} catch (error) {
			throw new Error(`font family "${family}": ${asError(error).message}`, { cause: error });
		}
	};
	for (const entry of value) {
		const font = props(entry, ["family", "regular", "bold", "italic", "boldItalic"]);
		if (typeof font.family !== "string" || !font.family.trim() || font.family.includes(","))
			throw new Error("font family must name one registered family");
		if (result.has(font.family.trim())) throw new Error(`duplicate font family: ${font.family}`);
		const face = (key: "regular" | "bold" | "italic" | "boldItalic") => {
			const bytes = font[key];
			if (bytes === undefined && key !== "regular") return undefined;
			if (!(bytes instanceof Uint8Array) || !bytes.byteLength)
				throw new Error(`font ${key} must be a non-empty Uint8Array`);
			return addFont(bytes, font.family as string);
		};
		result.set(font.family.trim(), {
			regular: face("regular")!,
			bold: face("bold"),
			italic: face("italic"),
			boldItalic: face("boldItalic"),
		});
	}
	return result;
}

/** The core reports a glyph it cannot show as `UnsupportedCharacter('x')` (a built-in font, WinAnsi
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
			: `the built-in font has no glyph for "${character}" (${codePoint}); register a font that has it`,
	);
}
