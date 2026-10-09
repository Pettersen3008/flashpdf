import assert from "node:assert/strict";

import { test } from "vitest";

import { registerFonts } from "../dist/fonts.js";

test("given fonts totalling over 64 MiB, when registering, then rejects the last one before copying it into WASM", () => {
	const copied = [];
	const renderer = { add_font: (bytes) => copied.push(bytes.byteLength) + 1 };
	const half = new Uint8Array(32 * 1024 * 1024 + 1);
	assert.throws(
		() => registerFonts(renderer, [{ family: "Big", regular: half, bold: half }]),
		/^Error: font family "Big": fonts exceed 64 MiB per render$/,
	);
	assert.deepEqual(copied, [half.byteLength]);
});
