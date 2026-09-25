import assert from "node:assert/strict";

import { test } from "vitest";

import { Binary } from "../dist/binary.js";
import { ProtocolWriter } from "../dist/protocol.js";

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

test("given a rich paragraph over 64 KiB, when writing it, then rejects with a split hint", () => {
	const memory = { buffer: new ArrayBuffer(4096) };
	const renderer = {
		input_ptr: () => 0,
		input_capacity: () => 4096,
		push: () => {},
		finish: () => new Uint8Array(),
	};
	const writer = new ProtocolWriter(new Binary(renderer, memory), renderer);
	const run = (font, text) => ({
		font,
		size: 12,
		color: [0, 0, 0],
		text,
		break: false,
		underline: false,
		lineThrough: false,
		link: undefined,
	});
	assert.throws(
		() => writer.paragraph([run(0, "a".repeat(40_000)), run(1, "b".repeat(40_000))], "left"),
		/paragraph exceeds 64 KiB; split it into several paragraphs/,
	);
});

test("given the same Uint8Array used for two images, when writing them, then registers it once", () => {
	const memory = { buffer: new ArrayBuffer(4096) };
	const registered = [];
	const renderer = {
		input_ptr: () => 0,
		input_capacity: () => 4096,
		push: () => {},
		finish: () => new Uint8Array(),
		add_image: (bytes) => registered.push(bytes) - 1,
	};
	const writer = new ProtocolWriter(new Binary(renderer, memory), renderer);
	const logo = new Uint8Array([1, 2, 3]);
	writer.image(logo, undefined, undefined, "", undefined);
	writer.image(logo, { kind: 1, value: 40 }, 20, "Logo", "https://example.com/");
	writer.image(new Uint8Array([1, 2, 3]), undefined, undefined, "", undefined);
	assert.equal(registered.length, 2);
	assert.equal(registered[0], logo);
});
