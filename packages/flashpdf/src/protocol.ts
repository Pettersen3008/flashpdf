import { asError, Binary } from "./binary.js";
import type { Box } from "./style.js";

export type Column = { kind: 0 | 1 | 2; value: number };
/** A table row's column: a cell that starts there, more of the cell to its left, or part of a cell from an earlier row. */
export type Slot = { align: 0 | 1 | 2; rows: number } | "join" | "covered";
/** An image dimension: 1 is points, 2 is percent of the container width. */
export type Dimension = { kind: 1 | 2; value: number };
/** Alignment codes: 0 auto, then stretch, start, end, center. */
export type Flex = {
	column: boolean;
	/** Start, end, center, space-between, space-around, space-evenly. */
	justify: number;
	align: number;
	gap: number;
	/** Bounds of the container's content height; equal when the height is fixed. */
	minHeight: number;
	maxHeight: number | undefined;
};
/** `basis` undefined is `auto`. */
export type FlexItem = {
	grow: number;
	shrink: number;
	basis: Dimension | undefined;
	align: number;
};
/** One styled stretch of a paragraph; `break` ends the line after it. */
export type Run = {
	font: number;
	size: number;
	/** Points; 0 is `normal`, the font's ascent, descent, and line gap. */
	lineHeight: number;
	color: readonly [number, number, number];
	text: string;
	break: boolean;
	underline: boolean;
	lineThrough: boolean;
	link: string | undefined;
};

const opcode = {
	positionStart: 28,
	positionEnd: 29,
	spacer: 2,
	stackStart: 3,
	stackEnd: 4,
	rowStart: 5,
	rowEnd: 6,
	flexStart: 23,
	flexEnd: 24,
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
	pdfa: 20,
	attachment: 21,
	avoid: 22,
	end: 255,
} as const;

export class ProtocolWriter {
	/** The render option whose header or footer block is being written. */
	private repeating: string | undefined;
	/** Slot per registered byte array, so a logo reused on every row embeds once. */
	private readonly images = new Map<Uint8Array, number>();

	constructor(
		private readonly binary: Binary,
		private readonly assets: { add_image(bytes: Uint8Array): number },
	) {}

	header(width: number, height: number, margin: readonly number[]) {
		this.binary.header(width, height, margin);
	}

	metadata(value: Record<string, unknown>, tagged: boolean) {
		this.binary.record(opcode.metadata, () => {
			this.binary.u8(tagged ? 1 : 0);
			for (const key of ["title", "author", "subject", "keywords", "language"])
				this.binary.text((value[key] as string | undefined) ?? "");
		});
	}

	/** Profile 0 is plain PDF/A-3b; 1 to 6 embed the Factur-X XML registered as `xml`. */
	pdfa(profile: number, xml: number) {
		this.binary.record(opcode.pdfa, () => {
			this.binary.u8(profile);
			if (profile) this.binary.u16(xml);
		});
	}

	attachment(slot: number, relationship: number, name: string, mime: string, description: string) {
		this.binary.record(opcode.attachment, () => {
			this.binary.u16(slot);
			this.binary.u8(relationship);
			this.binary.text(name);
			this.binary.text(mime);
			this.binary.text(description);
		});
	}

	paragraph(runs: readonly Run[], align: string, orphans: number, widows: number) {
		const alignment = align === "center" ? 1 : align === "right" ? 2 : 0;
		const links: string[] = [];
		for (const run of runs)
			if (run.link !== undefined && !links.includes(run.link)) links.push(run.link);
		if (links.length > 0xffff) throw new Error("paragraph has too many links");
		try {
			this.binary.record(opcode.paragraph, () => {
				this.binary.u8(alignment);
				this.binary.u16(orphans);
				this.binary.u16(widows);
				this.binary.u16(links.length);
				for (const link of links) this.binary.text(link);
				this.binary.u16(runs.length);
				for (const run of runs) {
					this.binary.u8(run.font);
					this.binary.f32(run.size);
					for (const channel of run.color) this.binary.u8(channel);
					// Bit 0 hard break, 1 underline, 2 line-through, 3 a u16 link index follows,
					// 4 an f32 line height follows.
					this.binary.u8(
						(run.break ? 1 : 0) |
							(run.underline ? 2 : 0) |
							(run.lineThrough ? 4 : 0) |
							(run.link === undefined ? 0 : 8) |
							(run.lineHeight ? 16 : 0),
					);
					if (run.link !== undefined) this.binary.u16(links.indexOf(run.link));
					if (run.lineHeight) this.binary.f32(run.lineHeight);
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
		this.binary.record(opcode.spacer, () => this.binary.f32(height));
	}

	stackStart(gap: number) {
		this.binary.record(opcode.stackStart, () => this.binary.f32(gap));
	}

	stackEnd() {
		this.binary.record(opcode.stackEnd);
	}

	/** Without `slots`, each column holds its own top-aligned cell. */
	rowStart(columns: readonly Column[], slots: readonly Slot[] = []) {
		this.binary.record(opcode.rowStart, () => {
			this.binary.u16(columns.length);
			for (const [index, column] of columns.entries()) {
				this.binary.u8(column.kind);
				this.binary.f32(column.value);
				// Bits 0-1 align a cell top, middle, or bottom, and bit 2 means a u16 row span follows;
				// 8 joins the cell to the left, and 16 marks a column a cell from an earlier row covers.
				const slot = slots[index] ?? { align: 0, rows: 1 };
				if (slot === "join") this.binary.u8(8);
				else if (slot === "covered") this.binary.u8(16);
				else if (slot.rows > 1) {
					this.binary.u8(4 | slot.align);
					this.binary.u16(slot.rows);
				} else this.binary.u8(slot.align);
			}
		});
	}

	rowEnd() {
		this.binary.record(opcode.rowEnd);
	}

	flexStart(flex: Flex, items: readonly FlexItem[]) {
		this.binary.record(opcode.flexStart, () => {
			this.binary.u8(flex.column ? 1 : 0);
			this.binary.u8(flex.justify);
			this.binary.u8(flex.align);
			this.binary.f32(flex.gap);
			this.binary.f32(flex.minHeight);
			this.height(flex.maxHeight);
			this.binary.u16(items.length);
			for (const item of items) {
				this.binary.f32(item.grow);
				this.binary.f32(item.shrink);
				this.size(item.basis);
				this.binary.u8(item.align);
			}
		});
	}

	flexEnd() {
		this.binary.record(opcode.flexEnd);
	}

	/** The first `headerRows` children repeat on every page the table continues on. */
	tableStart(headerRows: number, footerRows: number, width: Column) {
		if (headerRows > 0xffff) throw new Error("too many header rows");
		if (footerRows > 0xffff) throw new Error("too many footer rows");
		this.binary.record(opcode.tableStart, () => {
			this.binary.u16(headerRows);
			this.binary.u16(footerRows);
			this.binary.u8(width.kind);
			this.binary.f32(width.value);
		});
	}

	tableEnd() {
		this.binary.record(opcode.tableEnd);
	}

	pageBreak() {
		if (this.repeating) throw new Error(`${this.repeating} cannot break the page`);
		this.binary.record(opcode.pageBreak);
	}

	/** Avoids a page break here; a header or footer line has no breaks. */
	avoid() {
		if (!this.repeating) this.binary.record(opcode.avoid);
	}

	/** The engine takes the next top-level block, this stack, as the header or footer. */
	repeatStart(position: "header" | "footer", first: boolean, option: string) {
		this.binary.record(opcode[position], () => this.binary.u8(first ? 1 : 0));
		this.stackStart(0);
		this.repeating = option;
	}

	repeatEnd() {
		this.repeating = undefined;
		this.stackEnd();
	}

	boxStart(box: Box) {
		this.binary.record(opcode.boxStart, () => {
			for (const edge of [...box.margin, ...box.padding, ...box.border]) this.binary.f32(edge);
			if (box.background) {
				this.binary.u8(1);
				for (const channel of box.background) this.binary.u8(channel);
			} else this.binary.u8(0);
			for (const edge of box.borderColor) for (const channel of edge) this.binary.u8(channel);
			const size = box.size;
			this.binary.u8((size?.borderBox ? 1 : 0) | ((size?.autoMargin ?? 0) << 1));
			this.size(size?.width);
			this.size(size?.minWidth);
			this.size(size?.maxWidth);
			this.height(size?.height);
			this.height(size?.minHeight);
			this.height(size?.maxHeight);
		});
	}

	/** Kind 0 is `auto`. */
	private size(value: Dimension | undefined) {
		this.binary.u8(value?.kind ?? 0);
		if (value) this.binary.f32(value.value);
	}

	private height(value: number | undefined) {
		this.size(value === undefined ? undefined : { kind: 1, value });
	}

	positionStart(kind: string, offsets: readonly (Dimension | undefined)[]) {
		if (this.repeating && kind === "fixed")
			throw new Error("fixed positioning cannot repeat inside page furniture");
		this.binary.record(opcode.positionStart, () => {
			this.binary.u8(kind === "relative" ? 0 : kind === "absolute" ? 1 : 2);
			for (const value of offsets) this.size(value);
		});
	}

	positionEnd() {
		this.binary.record(opcode.positionEnd);
	}

	boxEnd() {
		this.binary.record(opcode.boxEnd);
	}

	end() {
		this.binary.record(opcode.end);
	}
}
