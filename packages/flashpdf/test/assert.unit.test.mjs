import assert from "node:assert/strict";

import { test } from "vitest";

import { text } from "../dist/assert.js";
import { glyphError } from "../dist/fonts.js";

test("given text outside WinAnsi, when validated at the boundary, then accepts it and rejects only control characters and lone surrogates", () => {
	assert.equal(text("Ærø 中文 😀 – €"), "Ærø 中文 😀 – €");
	assert.equal(text("tab\tand\nnewline"), "tab\tand\nnewline");
	for (const value of ["\u0007", "\u000b", "\u007f", "\u0085", "\ud83d"])
		assert.throws(() => text(value), /unsupported control character/, JSON.stringify(value));
});

test("given the core's glyph errors, when mapped, then names the character, code point and font", () => {
	assert.match(
		glyphError(new Error("MissingGlyph('中')")).message,
		/^the font has no glyph for "中" \(U\+4E2D\)$/,
	);
	assert.match(
		glyphError(new Error("UnsupportedCharacter('\\u{ad}')")).message,
		/^Helvetica has no glyph for "­" \(U\+00AD\); register a font that has it$/,
	);
	assert.equal(glyphError(new Error("PageOverflow")).message, "PageOverflow");
});
