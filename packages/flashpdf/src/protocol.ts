import { Binary } from "./binary.js";
import type { Box } from "./style.js";

export type Column = { kind: 0 | 1 | 2; value: number };
/** One styled stretch of a paragraph; `break` ends the line after it. */
export type Run = {
	font: number;
	size: number;
	color: readonly [number, number, number];
	text: string;
	break: boolean;
};

const opcode = {
	spacer: 2,
	stackStart: 3,
	stackEnd: 4,
	rowStart: 5,
	rowEnd: 6,
	pageBreak: 7,
	footer: 8,
	paragraph: 9,
	boxStart: 17,
	boxEnd: 18,
	end: 255,
} as const;

export class ProtocolWriter {
	private footer = false;
	private footerWritten = false;

	constructor(private readonly binary: Binary) {}

	header(width: number, height: number, margin: number) {
		this.binary.header(width, height, margin);
	}

	paragraph(runs: readonly Run[], align: string) {
		const alignment = align === "center" ? 1 : align === "right" ? 2 : 0;
		if (this.footer) {
			const [only] = runs;
			if (this.footerWritten || runs.length !== 1 || only!.break)
				throw new Error("footer must be a single text line");
			this.footerWritten = true;
			this.binary.record(opcode.footer, () => {
				this.binary.f32(only!.size);
				this.binary.u8(alignment);
				for (const channel of only!.color) this.binary.u8(channel);
				this.binary.u8(only!.font);
				this.binary.text(only!.text);
			});
			return;
		}
		try {
			this.binary.record(opcode.paragraph, () => {
				this.binary.u8(alignment);
				this.binary.u16(runs.length);
				for (const run of runs) {
					this.binary.u8(run.font);
					this.binary.f32(run.size);
					for (const channel of run.color) this.binary.u8(channel);
					this.binary.u8(run.break ? 1 : 0);
					this.binary.text(run.text);
				}
			});
		} catch (error) {
			if (error instanceof Error && error.message === "record too large")
				throw new Error("paragraph exceeds 64 KiB; split it into several paragraphs");
			throw error;
		}
	}

	spacer(height: number) {
		this.body();
		this.binary.record(opcode.spacer, () => this.binary.f32(height));
	}

	stackStart(gap: number) {
		this.body();
		this.binary.record(opcode.stackStart, () => this.binary.f32(gap));
	}

	stackEnd() {
		this.body();
		this.binary.record(opcode.stackEnd);
	}

	rowStart(columns: readonly Column[]) {
		this.body();
		this.binary.record(opcode.rowStart, () => {
			this.binary.u16(columns.length);
			for (const column of columns) {
				this.binary.u8(column.kind);
				this.binary.f32(column.value);
			}
		});
	}

	rowEnd() {
		this.body();
		this.binary.record(opcode.rowEnd);
	}

	pageBreak() {
		this.body();
		this.binary.record(opcode.pageBreak);
	}

	footerStart() {
		this.footer = true;
		this.footerWritten = false;
	}

	footerEnd() {
		if (!this.footerWritten) throw new Error("footer must be a single text line");
		this.footer = false;
	}

	boxStart(box: Box) {
		this.body();
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
		this.body();
		this.binary.record(opcode.boxEnd);
	}

	end() {
		this.binary.record(opcode.end);
	}

	private body() {
		if (this.footer) throw new Error("footer must be a single text line");
	}
}
