import assert from "node:assert/strict";
import { createHash } from "node:crypto";

import { test } from "vitest";

import { code128 } from "../dist/barcode.js";
import { codewords, qrCode } from "../dist/qr.js";

const rows = ({ size, dark }) =>
	Array.from({ length: size }, (_, y) => dark.subarray(y * size, y * size + size).join(""));
const sha256 = (lines) => createHash("sha256").update(lines.join("\n")).digest("hex");

test("given HELLO WORLD at level Q, when encoding, then codewords and modules match the worked example", () => {
	// Data and error correction codewords from thonky.com's QR tutorial; modules from Nayuki's qrcodegen 1.8.0.
	assert.deepEqual(codewords("HELLO WORLD", "Q"), {
		version: 1,
		codewords: [
			32, 91, 11, 120, 209, 114, 220, 77, 67, 64, 236, 17, 236, 168, 72, 22, 82, 217, 54, 156, 0,
			46, 15, 180, 122, 16,
		],
	});
	assert.deepEqual(rows(qrCode("HELLO WORLD", "Q")), [
		"111111101100001111111",
		"100000101001001000001",
		"101110101001101011101",
		"101110101000001011101",
		"101110101010001011101",
		"100000100010001000001",
		"111111101010101111111",
		"000000001000000000000",
		"011010110000101011111",
		"010000001111000010001",
		"001101110110001011000",
		"011011010011010101110",
		"100010101011101110101",
		"000000001101001000101",
		"111111101010000101100",
		"100000100101101101000",
		"101110101010001111111",
		"101110100101010100010",
		"101110101001011101001",
		"100000101011110001011",
		"111111100001011100001",
	]);
});

test("given values in each mode up to version 40, when encoding, then modules and mask match Nayuki's qrcodegen", () => {
	const epc =
		"BCD\n002\n1\nSCT\nBFSWDE33\nFranz Mustermänn\nDE71110220330123456789\nEUR12.30\nGDDS\nRF18539007547034\n\nInvoice 2026-0042";
	for (const [text, level, size, hash] of [
		[
			"Zürich Straße 12 – €",
			"L",
			25,
			"8f088025736c253d48055812d9f220ca0963572408adf345f67b42f6a75e16da",
		],
		[
			"$%*+-./: 0123456789",
			"Q",
			25,
			"882c2f754fe684e27f5871902577a1784515753e400e677af80d4dab41db7b78",
		],
		[epc, "M", 45, "b2744b7401b3e13e55931a3a30aa1ff8aed5228fca90d4f1f8c9b89457daa33c"],
		[
			"0123456789".repeat(100),
			"H",
			105,
			"319917ddbbfdeb0f60431ba6308cdb07e26eb3968ba64bf8b13c9792fc19882f",
		],
		[
			"x".repeat(2331),
			"M",
			177,
			"b79a2ade5122c29ec45a888fb0ff276e5ad3012fce208565ea30a61e39346e25",
		],
	]) {
		const symbol = qrCode(text, level);
		assert.equal(symbol.size, size);
		assert.equal(sha256(rows(symbol)), hash, `${text.slice(0, 20)} at ${level}`);
	}
	assert.throws(
		() => qrCode("x".repeat(2332), "M"),
		/needs more than version 40 at error correction M/,
	);
});

test("given text and digit runs, when encoding Code 128, then switches between code sets B and C with the checksum", () => {
	assert.deepEqual(code128("Wikipedia"), [104, 55, 73, 75, 73, 80, 69, 68, 73, 65, 88, 106]);
	assert.deepEqual(code128("0123456789"), [105, 1, 23, 45, 67, 89, 73, 106]);
	assert.deepEqual(
		code128("AB12345678CD"),
		[104, 33, 34, 99, 12, 34, 56, 78, 100, 35, 36, 90, 106],
	);
	// An odd run leaves its first digit in code set B, so C takes pairs.
	assert.deepEqual(code128("X12345"), [104, 56, 17, 99, 23, 45, 87, 106]);
	assert.throws(() => code128("Ål"), /printable ASCII/);
});
