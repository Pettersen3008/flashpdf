import { Binary } from "./binary.js";
import type { Box } from "./style.js";

export type Column = { kind: 0 | 1 | 2; value: number };

const opcode = {
	text: 1,
	spacer: 2,
	stackStart: 3,
	stackEnd: 4,
	rowStart: 5,
	rowEnd: 6,
	pageBreak: 7,
	styledText: 16,
	boxStart: 17,
	boxEnd: 18,
	end: 255,
} as const;

export class ProtocolWriter {
	constructor(private readonly binary: Binary) {}

	header(width: number, height: number, margin: number) {
		this.binary.header(width, height, margin);
	}

	text(
		value: string,
		size: number,
		align: string,
		color: readonly [number, number, number],
		font: number,
	) {
		if (align === "left" && color[0] === 0 && color[1] === 0 && color[2] === 0 && font === 0)
			this.binary.record(opcode.text, () => {
				this.binary.f32(size);
				this.binary.text(value);
			});
		else
			this.binary.record(opcode.styledText, () => {
				this.binary.f32(size);
				this.binary.u8(align === "center" ? 1 : align === "right" ? 2 : 0);
				for (const channel of color) this.binary.u8(channel);
				this.binary.u8(font);
				this.binary.text(value);
			});
	}

	spacer(height: number) {
		this.binary.record(opcode.spacer, () => this.binary.f32(height));
	}

	stackStart(gap: number) {
		this.binary.record(opcode.stackStart, () => this.binary.f32(gap));
	}

	stackEnd() {
		this.binary.record(opcode.stackEnd);
	}

	rowStart(columns: readonly Column[]) {
		this.binary.record(opcode.rowStart, () => {
			this.binary.u16(columns.length);
			for (const column of columns) {
				this.binary.u8(column.kind);
				this.binary.f32(column.value);
			}
		});
	}

	rowEnd() {
		this.binary.record(opcode.rowEnd);
	}

	pageBreak() {
		this.binary.record(opcode.pageBreak);
	}

	boxStart(box: Box) {
		this.binary.record(opcode.boxStart, () => {
			for (const edge of [...box.margin, ...box.padding, ...box.border]) this.binary.f32(edge);
			if (box.background) {
				this.binary.u8(1);
				for (const channel of box.background) this.binary.u8(channel);
			} else this.binary.u8(0);
			for (const edge of box.borderColor) for (const channel of edge) this.binary.u8(channel);
		});
	}

	boxEnd() {
		this.binary.record(opcode.boxEnd);
	}

	end() {
		this.binary.record(opcode.end);
	}
}
