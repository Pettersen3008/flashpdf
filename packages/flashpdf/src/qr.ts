/*
 * Ported from the QR Code generator library (TypeScript) by Project Nayuki,
 * https://www.nayuki.io/page/qr-code-generator-library, and trimmed to one
 * segment, a fixed error correction level, and automatic masking.
 *
 * Copyright (c) Project Nayuki. (MIT License)
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy of
 * this software and associated documentation files (the "Software"), to deal in
 * the Software without restriction, including without limitation the rights to
 * use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
 * the Software, and to permit persons to whom the Software is furnished to do so,
 * subject to the following conditions:
 * - The above copyright notice and this permission notice shall be included in
 *   all copies or substantial portions of the Software.
 * - The Software is provided "as is", without warranty of any kind, express or
 *   implied, including but not limited to the warranties of merchantability,
 *   fitness for a particular purpose and noninfringement. In no event shall the
 *   authors or copyright holders be liable for any claim, damages or other
 *   liability, whether in an action of contract, tort or otherwise, arising from,
 *   out of or in connection with the Software or the use or other dealings in the
 *   Software.
 */

export type Level = "L" | "M" | "Q" | "H";
/** Table row and format bits per level. */
const LEVELS: Record<Level, readonly [number, number]> = {
	L: [0, 1],
	M: [1, 0],
	Q: [2, 3],
	H: [3, 2],
};
// Indexed by level row, then version; index 0 is padding.
const ECC_CODEWORDS_PER_BLOCK = [
	[
		-1, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28, 30,
		30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
	],
	[
		-1, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28,
		28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
	],
	[
		-1, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30, 30,
		30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
	],
	[
		-1, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24, 30,
		30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
	],
];
const NUM_ERROR_CORRECTION_BLOCKS = [
	[
		-1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13, 14,
		15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25,
	],
	[
		-1, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23,
		25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49,
	],
	[
		-1, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27, 29, 34,
		34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68,
	],
	[
		-1, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32, 35,
		37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81,
	],
];
const ALPHANUMERIC = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

function rawModules(version: number): number {
	let result = (16 * version + 128) * version + 64;
	if (version >= 2) {
		const align = Math.floor(version / 7) + 2;
		result -= (25 * align - 10) * align - 55;
		if (version >= 7) result -= 36;
	}
	return result;
}

function dataCodewords(version: number, row: number): number {
	return (
		Math.floor(rawModules(version) / 8) -
		ECC_CODEWORDS_PER_BLOCK[row]![version]! * NUM_ERROR_CORRECTION_BLOCKS[row]![version]!
	);
}

function append(bits: number[], value: number, length: number) {
	for (let index = length - 1; index >= 0; index--) bits.push((value >>> index) & 1);
}

/** Numeric or alphanumeric mode when the whole value fits it, otherwise UTF-8 bytes. */
function segment(text: string) {
	const bits: number[] = [];
	if (/^\d+$/.test(text)) {
		for (let index = 0; index < text.length; index += 3) {
			const digits = text.slice(index, index + 3);
			append(bits, Number(digits), digits.length * 3 + 1);
		}
		return { mode: 1, count: text.length, bits, widths: [10, 12, 14] };
	}
	if ([...text].every((character) => ALPHANUMERIC.includes(character))) {
		for (let index = 0; index < text.length; index += 2) {
			const pair = text.slice(index, index + 2);
			const first = ALPHANUMERIC.indexOf(pair[0]!);
			if (pair.length === 2) append(bits, first * 45 + ALPHANUMERIC.indexOf(pair[1]!), 11);
			else append(bits, first, 6);
		}
		return { mode: 2, count: text.length, bits, widths: [9, 11, 13] };
	}
	const bytes = new TextEncoder().encode(text);
	for (const byte of bytes) append(bits, byte, 8);
	return { mode: 4, count: bytes.length, bits, widths: [8, 16, 16] };
}

function multiply(x: number, y: number): number {
	let z = 0;
	for (let index = 7; index >= 0; index--) {
		z = (z << 1) ^ ((z >>> 7) * 0x11d);
		z ^= ((y >>> index) & 1) * x;
	}
	return z;
}

function divisor(degree: number): number[] {
	const result: number[] = Array.from({ length: degree }, (_, index) =>
		index === degree - 1 ? 1 : 0,
	);
	let root = 1;
	for (let step = 0; step < degree; step++) {
		for (let index = 0; index < result.length; index++) {
			result[index] = multiply(result[index]!, root);
			if (index + 1 < result.length) result[index]! ^= result[index + 1]!;
		}
		root = multiply(root, 0x02);
	}
	return result;
}

function remainder(data: readonly number[], generator: readonly number[]): number[] {
	const result = generator.map(() => 0);
	for (const byte of data) {
		const factor = byte ^ result.shift()!;
		result.push(0);
		generator.forEach((coefficient, index) => (result[index]! ^= multiply(coefficient, factor)));
	}
	return result;
}

/** Data codewords followed by their Reed-Solomon codewords, interleaved by block. */
export function codewords(text: string, level: Level): { version: number; codewords: number[] } {
	const row = LEVELS[level][0];
	const { mode, count, bits: payload, widths } = segment(text);
	let version = 1;
	let width = widths[0]!;
	for (; ; version++) {
		if (version > 40)
			throw new Error(
				`QRCode value needs more than version 40 at error correction ${level}; shorten it or lower the level`,
			);
		width = widths[Math.floor((version + 7) / 17)]!;
		if (count < 2 ** width && 4 + width + payload.length <= dataCodewords(version, row) * 8) break;
	}
	const capacity = dataCodewords(version, row) * 8;
	const bits: number[] = [];
	append(bits, mode, 4);
	append(bits, count, width);
	bits.push(...payload);
	append(bits, 0, Math.min(4, capacity - bits.length));
	append(bits, 0, (8 - (bits.length % 8)) % 8);
	for (let pad = 0xec; bits.length < capacity; pad ^= 0xec ^ 0x11) append(bits, pad, 8);
	const data = Array.from({ length: bits.length / 8 }, (_, index) =>
		bits.slice(index * 8, index * 8 + 8).reduce((byte, bit) => (byte << 1) | bit, 0),
	);

	const blocks = NUM_ERROR_CORRECTION_BLOCKS[row]![version]!;
	const eccLength = ECC_CODEWORDS_PER_BLOCK[row]![version]!;
	const raw = Math.floor(rawModules(version) / 8);
	const shortBlocks = blocks - (raw % blocks);
	const shortLength = Math.floor(raw / blocks);
	const generator = divisor(eccLength);
	const parts: number[][] = [];
	for (let index = 0, offset = 0; index < blocks; index++) {
		const block = data.slice(
			offset,
			offset + shortLength - eccLength + (index < shortBlocks ? 0 : 1),
		);
		offset += block.length;
		const ecc = remainder(block, generator);
		if (index < shortBlocks) block.push(0);
		parts.push([...block, ...ecc]);
	}
	const result: number[] = [];
	for (let index = 0; index < parts[0]!.length; index++)
		parts.forEach((block, number) => {
			// Short blocks carry a placeholder where long blocks have one more data codeword.
			if (index !== shortLength - eccLength || number >= shortBlocks) result.push(block[index]!);
		});
	return { version, codewords: result };
}

const MASKS: readonly ((x: number, y: number) => boolean)[] = [
	(x, y) => (x + y) % 2 === 0,
	(_, y) => y % 2 === 0,
	(x) => x % 3 === 0,
	(x, y) => (x + y) % 3 === 0,
	(x, y) => (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0,
	(x, y) => ((x * y) % 2) + ((x * y) % 3) === 0,
	(x, y) => (((x * y) % 2) + ((x * y) % 3)) % 2 === 0,
	(x, y) => (((x + y) % 2) + ((x * y) % 3)) % 2 === 0,
];

/** The QR symbol for `text`: `size` modules a side, `dark[y * size + x]` set for a dark module. */
export function qrCode(text: string, level: Level): { size: number; dark: Uint8Array } {
	const { version, codewords: all } = codewords(text, level);
	const size = version * 4 + 17;
	const dark = new Uint8Array(size * size);
	const fixed = new Uint8Array(size * size);
	const set = (x: number, y: number, value: boolean) => {
		dark[y * size + x] = value ? 1 : 0;
		fixed[y * size + x] = 1;
	};
	const bit = (value: number, index: number) => ((value >>> index) & 1) !== 0;

	for (let index = 0; index < size; index++) {
		set(6, index, index % 2 === 0);
		set(index, 6, index % 2 === 0);
	}
	for (const [cx, cy] of [
		[3, 3],
		[size - 4, 3],
		[3, size - 4],
	] as const)
		for (let dy = -4; dy <= 4; dy++)
			for (let dx = -4; dx <= 4; dx++) {
				const distance = Math.max(Math.abs(dx), Math.abs(dy));
				const [x, y] = [cx + dx, cy + dy];
				if (x >= 0 && x < size && y >= 0 && y < size) set(x, y, distance !== 2 && distance !== 4);
			}
	const positions: number[] = [];
	if (version > 1) {
		const count = Math.floor(version / 7) + 2;
		const step = Math.floor((version * 8 + count * 3 + 5) / (count * 4 - 4)) * 2;
		positions.push(6);
		for (let position = size - 7; positions.length < count; position -= step)
			positions.splice(1, 0, position);
	}
	for (const [i, x] of positions.entries())
		for (const [j, y] of positions.entries()) {
			const last = positions.length - 1;
			if ((i === 0 && j === 0) || (i === 0 && j === last) || (i === last && j === 0)) continue;
			for (let dy = -2; dy <= 2; dy++)
				for (let dx = -2; dx <= 2; dx++)
					set(x + dx, y + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1);
		}
	const format = (mask: number) => {
		const data = (LEVELS[level][1] << 3) | mask;
		let rem = data;
		for (let index = 0; index < 10; index++) rem = (rem << 1) ^ ((rem >>> 9) * 0x537);
		const bits = ((data << 10) | rem) ^ 0x5412;
		for (let index = 0; index <= 5; index++) set(8, index, bit(bits, index));
		set(8, 7, bit(bits, 6));
		set(8, 8, bit(bits, 7));
		set(7, 8, bit(bits, 8));
		for (let index = 9; index < 15; index++) set(14 - index, 8, bit(bits, index));
		for (let index = 0; index < 8; index++) set(size - 1 - index, 8, bit(bits, index));
		for (let index = 8; index < 15; index++) set(8, size - 15 + index, bit(bits, index));
		set(8, size - 8, true);
	};
	format(0);
	if (version >= 7) {
		let rem = version;
		for (let index = 0; index < 12; index++) rem = (rem << 1) ^ ((rem >>> 11) * 0x1f25);
		const bits = (version << 12) | rem;
		for (let index = 0; index < 18; index++) {
			const [a, b] = [size - 11 + (index % 3), Math.floor(index / 3)];
			set(a, b, bit(bits, index));
			set(b, a, bit(bits, index));
		}
	}

	let index = 0;
	for (let right = size - 1; right >= 1; right -= 2) {
		if (right === 6) right = 5;
		for (let vertical = 0; vertical < size; vertical++)
			for (let column = 0; column < 2; column++) {
				const x = right - column;
				const y = ((right + 1) & 2) === 0 ? size - 1 - vertical : vertical;
				if (!fixed[y * size + x] && index < all.length * 8) {
					dark[y * size + x] = bit(all[index >>> 3]!, 7 - (index & 7)) ? 1 : 0;
					index++;
				}
			}
	}

	const apply = (mask: number) => {
		for (let y = 0; y < size; y++)
			for (let x = 0; x < size; x++)
				if (!fixed[y * size + x] && MASKS[mask]!(x, y)) dark[y * size + x]! ^= 1;
	};
	let best = 0;
	let lowest = Infinity;
	for (let mask = 0; mask < 8; mask++) {
		apply(mask);
		format(mask);
		const score = penalty(dark, size);
		if (score < lowest) [best, lowest] = [mask, score];
		apply(mask);
	}
	apply(best);
	format(best);
	return { size, dark };
}

/** The spec's mask penalty: long runs, 2x2 blocks, finder-like patterns, and dark balance. */
function penalty(dark: Uint8Array, size: number): number {
	let result = 0;
	const add = (length: number, history: number[]) => {
		if (history[0] === 0) length += size;
		history.pop();
		history.unshift(length);
	};
	const finders = (history: readonly number[]) => {
		const n = history[1]!;
		const core =
			n > 0 && history[2] === n && history[3] === n * 3 && history[4] === n && history[5] === n;
		return (
			(core && history[0]! >= n * 4 && history[6]! >= n ? 1 : 0) +
			(core && history[6]! >= n * 4 && history[0]! >= n ? 1 : 0)
		);
	};
	for (const vertical of [false, true])
		for (let line = 0; line < size; line++) {
			let color = 0;
			let run = 0;
			const history = [0, 0, 0, 0, 0, 0, 0];
			for (let step = 0; step < size; step++) {
				const module = vertical ? dark[step * size + line]! : dark[line * size + step]!;
				if (module === color) {
					run++;
					if (run === 5) result += 3;
					else if (run > 5) result++;
				} else {
					add(run, history);
					if (!color) result += finders(history) * 40;
					color = module;
					run = 1;
				}
			}
			if (color) {
				add(run, history);
				run = 0;
			}
			add(run + size, history);
			result += finders(history) * 40;
		}
	let count = 0;
	for (let y = 0; y < size; y++)
		for (let x = 0; x < size; x++) {
			const module = dark[y * size + x]!;
			count += module;
			if (
				x < size - 1 &&
				y < size - 1 &&
				module === dark[y * size + x + 1] &&
				module === dark[(y + 1) * size + x] &&
				module === dark[(y + 1) * size + x + 1]
			)
				result += 3;
		}
	const total = size * size;
	return result + (Math.ceil(Math.abs(count * 20 - total * 10) / total) - 1) * 10;
}
