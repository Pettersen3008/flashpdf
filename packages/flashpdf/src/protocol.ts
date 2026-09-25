import { asError, Binary } from "./binary.js";
import type { Box } from "./style.js";

export type Column = { kind: 0 | 1 | 2; value: number };
/** An image dimension: 1 is points, 2 is percent of the container width. */
export type Dimension = { kind: 1 | 2; value: number };
/** One styled stretch of a paragraph; `break` ends the line after it. */
export type Run = {
	font: number;
	size: number;
	color: readonly [number, number, number];
	text: string;
	break: boolean;
	underline: boolean;
	lineThrough: boolean;
	link: string | undefined;
};

const opcode = {
	spacer: 2,
	stackStart: 3,
	stackEnd: 4,
	rowStart: 5,
	rowEnd: 6,
	pageBreak: 7,
	footer: 8,
	header: 13,
	metadata: 14,
	paragraph: 9,
	tableStart: 10,
	tableEnd: 11,
	image: 12,
	boxStart: 17,
	boxEnd: 18,
	end: 255,
} as const;

export class ProtocolWriter {
	private repeating: "header" | "footer" | undefined;
	private repeatWritten = false;
	/** Slot per registered byte array, so a logo reused on every row embeds once. */
	private readonly images = new Map<Uint8Array, number>();

	constructor(
		private readonly binary: Binary,
		private readonly assets: { add_image(bytes: Uint8Array): number },
	) {}

	header(width: number, height: number, margin: number) {
		this.binary.header(width, height, margin);
	}

	metadata(value: Record<string, unknown>, tagged: boolean) {
		this.binary.record(opcode.metadata, () => {
			this.binary.u8(tagged ? 1 : 0);
			for (const key of ["title", "author", "subject", "keywords", "language"])
				this.binary.text((value[key] as string | undefined) ?? "");
		});
	}

	paragraph(runs: readonly Run[], align: string) {
		const alignment = align === "center" ? 1 : align === "right" ? 2 : 0;
		if (this.repeating) {
			const [only] = runs;
			if (this.repeatWritten || runs.length !== 1 || only!.break)
				throw new Error(`${this.repeating} must be a single text line`);
			if (only!.underline || only!.lineThrough || only!.link !== undefined)
				throw new Error(`${this.repeating} text cannot be underlined or linked`);
			this.repeatWritten = true;
			this.binary.record(opcode[this.repeating], () => {
				this.binary.f32(only!.size);
				this.binary.u8(alignment);
				for (const channel of only!.color) this.binary.u8(channel);
				this.binary.u8(only!.font);
				this.binary.text(only!.text);
			});
			return;
		}
		const links: string[] = [];
		for (const run of runs)
			if (run.link !== undefined && !links.includes(run.link)) links.push(run.link);
		if (links.length > 0xffff) throw new Error("paragraph has too many links");
		try {
			this.binary.record(opcode.paragraph, () => {
				this.binary.u8(alignment);
				this.binary.u16(links.length);
				for (const link of links) this.binary.text(link);
				this.binary.u16(runs.length);
				for (const run of runs) {
					this.binary.u8(run.font);
					this.binary.f32(run.size);
					for (const channel of run.color) this.binary.u8(channel);
					// Bit 0 hard break, 1 underline, 2 line-through, 3 a u16 link index follows.
					this.binary.u8(
						(run.break ? 1 : 0) |
							(run.underline ? 2 : 0) |
							(run.lineThrough ? 4 : 0) |
							(run.link === undefined ? 0 : 8),
					);
					if (run.link !== undefined) this.binary.u16(links.indexOf(run.link));
					this.binary.text(run.text);
				}
			});
		} catch (error) {
			if (error instanceof Error && error.message === "record too large")
				throw new Error("paragraph exceeds 64 KiB; split it into several paragraphs");
			throw error;
		}
	}

	image(
		src: Uint8Array,
		width: Dimension | undefined,
		height: number | undefined,
		alt: string,
		link: string | undefined,
	) {
		this.body();
		let slot = this.images.get(src);
		if (slot === undefined) {
			try {
				slot = this.assets.add_image(src);
			} catch (error) {
				throw asError(error);
			}
			this.images.set(src, slot);
		}
		this.binary.record(opcode.image, () => {
			this.binary.u16(slot);
			if (width) {
				this.binary.u8(width.kind);
				this.binary.f32(width.value);
			} else this.binary.u8(0);
			if (height === undefined) this.binary.u8(0);
			else {
				this.binary.u8(1);
				this.binary.f32(height);
			}
			if (link === undefined) this.binary.u8(0);
			else {
				this.binary.u8(1);
				this.binary.text(link);
			}
			this.binary.text(alt);
		});
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

	/** The first `headerRows` children repeat on every page the table continues on. */
	tableStart(headerRows: number, width: Column) {
		this.body();
		if (headerRows > 0xffff) throw new Error("too many header rows");
		this.binary.record(opcode.tableStart, () => {
			this.binary.u16(headerRows);
			this.binary.u8(width.kind);
			this.binary.f32(width.value);
		});
	}

	tableEnd() {
		this.body();
		this.binary.record(opcode.tableEnd);
	}

	pageBreak() {
		this.body();
		this.binary.record(opcode.pageBreak);
	}

	repeatStart(position: "header" | "footer") {
		this.repeating = position;
		this.repeatWritten = false;
	}

	repeatEnd() {
		if (!this.repeatWritten) throw new Error(`${this.repeating} must be a single text line`);
		this.repeating = undefined;
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
		if (this.repeating) throw new Error(`${this.repeating} must be a single text line`);
	}
}
