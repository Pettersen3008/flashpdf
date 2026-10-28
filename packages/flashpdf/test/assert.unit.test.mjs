import assert from "node:assert/strict";

import { test } from "vitest";

import { text } from "../dist/assert.js";

test("given text outside WinAnsi, when validated at the boundary, then accepts it and rejects only control characters and lone surrogates", () => {
	assert.equal(text("Ærø 中文 😀 – €"), "Ærø 中文 😀 – €");
	assert.equal(text("tab\tand\nnewline"), "tab\tand\nnewline");
	for (const value of ["\u0007", "\u000b", "\u007f", "\u0085", "\ud83d"])
		assert.throws(() => text(value), /unsupported control character/, JSON.stringify(value));
});
