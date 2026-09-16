import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";

import { test } from "vitest";

import { PdfRenderer, initSync } from "../packages/flashpdf/wasm/flashpdf_wasm.js";

const encoder = new TextEncoder();
mkdirSync(new URL("../dist/", import.meta.url), { recursive: true });

function join(...parts) {
	const bytes = new Uint8Array(parts.reduce((length, part) => length + part.length, 0));
	let offset = 0;
	for (const part of parts) {
		bytes.set(part, offset);
		offset += part.length;
	}
	return bytes;
}

function u16(value) {
	const bytes = new Uint8Array(2);
	new DataView(bytes.buffer).setUint16(0, value, true);
	return bytes;
}

function u32(value) {
	const bytes = new Uint8Array(4);
	new DataView(bytes.buffer).setUint32(0, value, true);
	return bytes;
}

function f32(value) {
	const bytes = new Uint8Array(4);
	new DataView(bytes.buffer).setFloat32(0, value, true);
	return bytes;
}

function string(value) {
	const bytes = encoder.encode(value);
	return join(u32(bytes.length), bytes);
}

function record(opcode, payload = new Uint8Array()) {
	return join(Uint8Array.of(opcode), u32(payload.length), payload);
}

function protocolHeader() {
	return join(encoder.encode("FPDF"), u16(1), f32(120), f32(60), f32(10));
}

function text(value, size = 10) {
	return record(1, join(f32(size), string(value)));
}

function row(left, right) {
	return join(
		record(5, join(u16(2), Uint8Array.of(0), f32(50), Uint8Array.of(1), f32(1))),
		text(left),
		text(right),
		record(6),
	);
}

function pushBytes(renderer, wasm, bytes) {
	let offset = 0;
	while (offset < bytes.length) {
		const length = Math.min(renderer.input_capacity(), bytes.length - offset);
		const window = new Uint8Array(
			wasm.memory.buffer,
			renderer.input_ptr(),
			renderer.input_capacity(),
		);
		window.set(bytes.subarray(offset, offset + length));
		renderer.push(length);
		offset += length;
	}
}

function renderProtocol(wasm, parts) {
	const renderer = new PdfRenderer();
	for (const part of parts) pushBytes(renderer, wasm, part);
	return renderer.finish();
}

function loadWasm() {
	return initSync({
		module: readFileSync(
			new URL("../packages/flashpdf/wasm/flashpdf_wasm_bg.wasm", import.meta.url),
		),
	});
}

function validate(pdf, name, pages = 1) {
	const output = new URL(`../dist/${name}.pdf`, import.meta.url);
	writeFileSync(output, pdf);
	const parsed = spawnSync(
		"cargo",
		[
			"run",
			"--locked",
			"--quiet",
			"-p",
			"flashpdf-core",
			"--example",
			"validate_pdf",
			"--",
			output.pathname,
			String(pages),
		],
		{ encoding: "utf8" },
	);
	assert.equal(parsed.status, 0, parsed.stderr);
	return parsed.stdout;
}

test("given protocol bytes, when streamed through the fixed window, then every split renders identically", () => {
	const wasm = loadWasm();
	const stream = join(protocolHeader(), text("Name"), row("row", "1"), record(255));
	const expected = renderProtocol(wasm, [stream]);
	assert.deepEqual(expected, renderProtocol(wasm, [stream]));
	for (let split = 0; split <= stream.length; split += 1) {
		assert.deepEqual(
			renderProtocol(wasm, [stream.subarray(0, split), stream.subarray(split)]),
			expected,
			`split ${split}`,
		);
	}
	assert.equal(validate(expected, "protocol-wasm"), "Name\nrow\n1");
});

test("given invalid protocol input, when pushed, then the release WASM reports the decoder error", () => {
	const wasm = loadWasm();
	for (const [stream, message] of [
		[join(protocolHeader(), record(42)), /unknown opcode/],
		[join(protocolHeader(), text("🙂")), /unsupported WinAnsi character/],
		[join(protocolHeader(), text("word ".repeat(100))), /PageOverflow/],
		[join(encoder.encode("XPDF"), u16(1), f32(120), f32(60), f32(10)), /invalid magic/],
	]) {
		assert.throws(() => renderProtocol(wasm, [stream]), message);
	}
	assert.throws(() => renderProtocol(wasm, [protocolHeader()]), /truncated protocol/);
});
