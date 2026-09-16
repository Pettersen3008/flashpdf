import assert from "node:assert/strict";

import { test } from "vitest";

import { Binary } from "../dist/binary.js";

test("given a small backend window, when writing a record, then streams every byte in order", () => {
	const memory = { buffer: new ArrayBuffer(3) };
	const chunks = [];
	const renderer = {
		input_ptr: () => 0,
		input_capacity: () => 3,
		push: (length) => chunks.push(...new Uint8Array(memory.buffer, 0, length)),
		finish: () => new Uint8Array(),
	};

	new Binary(renderer, memory).record(7);

	assert.deepEqual(chunks, [7, 0, 0, 0, 0]);
});
