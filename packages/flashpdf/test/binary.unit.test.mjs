import assert from "node:assert/strict";

import { test } from "vitest";

import { Binary } from "../dist/binary.js";

test("given a small backend window, when writing a record, then streams every byte in order", () => {
	const memory = { buffer: new ArrayBuffer(3) };
	const chunks = [];
	let pointerReads = 0;
	let capacityReads = 0;
	const renderer = {
		input_ptr: () => (pointerReads++, 0),
		input_capacity: () => (capacityReads++, 3),
		push: (length) => {
			chunks.push(...new Uint8Array(memory.buffer, 0, length));
			memory.buffer = new ArrayBuffer(3);
		},
		finish: () => new Uint8Array(),
	};

	const binary = new Binary(renderer, memory);
	binary.record(7, () => binary.u8(9));

	assert.deepEqual(chunks, [7, 1, 0, 0, 0, 9]);
	assert.equal(pointerReads, 1);
	assert.equal(capacityReads, 1);
});
