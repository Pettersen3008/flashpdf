import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { test } from "vitest";

import { Binary } from "../dist/binary.js";
import { ProtocolWriter } from "../dist/protocol.js";

test("given a small backend window, when flushing a queued record, then streams every byte in order", () => {
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
	assert.deepEqual(chunks, []);
	binary.flush();

	assert.deepEqual(chunks, [7, 1, 0, 0, 0, 9]);
	assert.equal(pointerReads, 1);
	assert.equal(capacityReads, 1);
});

const window = (staged) => ({
	input_ptr: () => 0,
	input_capacity: () => 4096,
	push: () => {},
	finish: () => new Uint8Array(),
	stage_image: (bytes) => staged.push(bytes) - 1,
});
const img = (src) => ({ kind: "element", tag: "img", classes: [], children: [], src });

test("given the same Uint8Array used for two images, when writing them, then stages it once", () => {
	const staged = [];
	const renderer = window(staged);
	const writer = new ProtocolWriter(
		new Binary(renderer, { buffer: new ArrayBuffer(4096) }),
		renderer,
	);
	const logo = new Uint8Array([1, 2, 3]);
	writer.block(0, [img(logo), img(logo), img(new Uint8Array([1, 2, 3]))]);
	assert.equal(staged.length, 2);
	assert.equal(staged[0], logo);
});

test("given images totalling over 64 MiB, when writing them, then rejects the last one before copying it into WASM", () => {
	const staged = [];
	const renderer = window(staged);
	const writer = new ProtocolWriter(
		new Binary(renderer, { buffer: new ArrayBuffer(4096) }),
		renderer,
	);
	const half = new Uint8Array(32 * 1024 * 1024 + 1);
	assert.throws(
		() => writer.block(0, [img(half), img(half.slice())]),
		/^Error: images exceed 64 MiB per render$/,
	);
	assert.deepEqual(
		staged.map((bytes) => bytes.byteLength),
		[half.byteLength],
	);
});

test("given a WASM fetch that fails once, when loading again, then retries instead of keeping the failure", async () => {
	const bytes = readFileSync(new URL("../wasm/flashpdf_wasm_bg.wasm", import.meta.url));
	const original = globalThis.fetch;
	let fetches = 0;
	globalThis.fetch = async () => {
		if (fetches++ === 0) throw new TypeError("network down");
		return new Response(bytes, { headers: { "Content-Type": "application/wasm" } });
	};
	try {
		const { loadBackend } = await import("../dist/wasm.browser.js");
		await assert.rejects(loadBackend(), /network down/);
		assert.ok((await loadBackend()).memory);
		assert.equal(fetches, 2);
	} finally {
		globalThis.fetch = original;
	}
});
