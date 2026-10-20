import { asError, Binary } from "./binary.js";
import type { Box, Transform, WhiteSpace } from "./style.js";

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
	/** Undefined is the text colour. */
	decorationColor: readonly [number, number, number] | undefined;
	link: string | undefined;
	/** 0 to 1, with every ancestor's opacity multiplied in. */
	opacity: number;
	/** Slant the upright face: the family has no italic face for this weight. */
	oblique: boolean;
	letterSpacing: number;
	wordSpacing: number;
	kerning: boolean;
	/** Baseline shift in points, up positive. */
	rise: number;
	/** The background behind the run's text, as `<mark>` paints it; undefined is none. */
	background: readonly [number, number, number] | undefined;
	/** 1 hangs a list marker before the paragraph's first line and 2 starts the line with it. */
	marker: 0 | 1 | 2;
	/** Applied by the compiler before writing; only `pre` and `nowrap` reach the engine, as a flag. */
	whiteSpace: WhiteSpace;
	/** Applied by the compiler before writing; not part of the protocol. */
	transform: Transform;
};

/** Run flag bits 5 to 7: slant, then an f32 letter and word spacing follow; bit 9: no kerning. */
function typography(run: Run): number {
	return (
		(run.oblique ? 32 : 0) |
		(run.letterSpacing ? 64 : 0) |
		(run.wordSpacing ? 128 : 0) |
		(run.kerning ? 0 : 512)
	);
}

/** The engine's alpha byte; 255 is opaque. */
const alpha = (opacity: number) => Math.round(opacity * 255);

const opcode = {
	spacer: 2,
	stackStart: 3,
	stackEnd: 4,
	rowStart: 5,
	rowEnd: 6,
	flexStart: 24,
	flexEnd: 25,
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
	anchor: 22,
	avoid: 23,
	end: 255,
} as const;
/** The core's caps; images are checked here because `add_image` copies the bytes into WASM memory. */
const MAX_IMAGE_BYTES = 64 * 1024 * 1024;
const MAX_PAGES = 10_000;

export class ProtocolWriter {
	/** The render option whose header or footer block is being written. */
	private repeating: string | undefined;
	/** Slot per registered byte array, so a logo reused on every row embeds once. */
	private readonly images = new Map<Uint8Array, number>();
	private imageBytes = 0;
	private pages = 1;
	/** Drawings are encoded per element, so an icon on every row needs a key by content. */
	private readonly drawings = new Map<string, Uint8Array>();
	private readonly latin1 = new TextDecoder("latin1");
	private held = 0;
	/** Box and stack starts waiting for content, so a held margin lands outside them. */
	private readonly starts: (() => void)[] = [];

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

	/** A destination at the top of the next line laid out. `level` 1 to 6 also makes it a bookmark. */
	anchor(name: string, level: number, title: string) {
		this.body(false);
		this.binary.record(opcode.anchor, () => {
			this.binary.text(name);
			this.binary.u8(level);
			this.binary.text(title);
		});
	}

	paragraph(runs: readonly Run[], align: string, orphans = 2, widows = 2) {
		const alignment = ["left", "center", "right", "justify"].indexOf(align);
		this.body();
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
					// 4 an f32 line height follows, 5 to 7, 9, and 11 to 14 typography, 8 a decoration
					// colour follows, 10 a background colour, 11 an f32 baseline shift, 12 no wrap,
					// and 13 and 14 a marker.
					const translucent = alpha(run.opacity) !== 255;
					this.binary.u16(
						(run.break ? 1 : 0) |
							(run.underline ? 2 : 0) |
							(run.lineThrough ? 4 : 0) |
							(run.link === undefined ? 0 : 8) |
							(run.lineHeight ? 16 : 0) |
							typography(run) |
							(translucent ? 32768 : 0) |
							(run.decorationColor ? 256 : 0) |
							(run.background ? 1024 : 0) |
							(run.rise ? 2048 : 0) |
							(run.whiteSpace === "pre" || run.whiteSpace === "nowrap" ? 4096 : 0) |
							(run.marker << 13),
					);
					if (run.link !== undefined) this.binary.u16(links.indexOf(run.link));
					if (run.lineHeight) this.binary.f32(run.lineHeight);
					this.spacing(run);
					if (run.rise) this.binary.f32(run.rise);
					for (const channel of run.decorationColor ?? []) this.binary.u8(channel);
					for (const channel of run.background ?? []) this.binary.u8(channel);
					if (translucent) this.binary.u8(alpha(run.opacity));
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
		opacity: number,
	) {
		let slot = this.images.get(src);
		if (slot === undefined) {
			if (this.imageBytes + src.byteLength > MAX_IMAGE_BYTES)
				throw new Error("images exceed 64 MiB per render");
			// The engine checks a new image against the records before it (PDF/A rejects CMYK).
			this.binary.flush();
			try {
				slot = this.assets.add_image(src);
			} catch (error) {
				throw asError(error);
			}
			this.imageBytes += src.byteLength;
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
			this.binary.u8(alpha(opacity));
		});
	}

	/** The first array with these bytes, which `image` has registered once. */
	drawing(bytes: Uint8Array): Uint8Array {
		const key = this.latin1.decode(bytes);
		const first = this.drawings.get(key);
		if (first) return first;
		this.drawings.set(key, bytes);
		return bytes;
	}

	spacer(height: number) {
		this.binary.record(opcode.spacer, () => this.binary.f32(height));
	}

	/** Holds a vertical margin until content follows, so adjoining margins collapse to the largest (CSS 2.1 8.3.1). */
	margin(height: number) {
		this.body(false);
		this.held = Math.max(this.held, height);
	}

	/** Writes the held margin and starts, so no margin collapses across this point. */
	flush() {
		if (this.held) {
			const held = this.held;
			this.binary.record(opcode.spacer, () => this.binary.f32(held));
		}
		this.held = 0;
		for (const start of this.starts.splice(0)) start();
	}

	/** `through` works as for `boxStart`. */
	stackStart(gap: number, through = false) {
		this.open(through, () => this.binary.record(opcode.stackStart, () => this.binary.f32(gap)));
	}

	stackEnd(through = false) {
		this.close(through, opcode.stackEnd);
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
		if (this.repeating) throw new Error(`${this.repeating} cannot contain page breaks`);
		this.held = 0;
		this.body();
		if (++this.pages > MAX_PAGES)
			throw new Error(`document exceeds ${MAX_PAGES} pages; split it across several renders`);
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

	/** With `through`, the start waits for content so a held margin collapses past it to land outside
	 *  the box, and the end lets the last child's margin out; an empty box writes nothing. */
	boxStart(box: Box, through = false) {
		this.open(through, () =>
			this.binary.record(opcode.boxStart, () => {
				for (const edge of [...box.margin, ...box.padding, ...box.border]) this.binary.f32(edge);
				if (box.background) {
					this.binary.u8(1);
					for (const channel of box.background) this.binary.u8(channel);
				} else this.binary.u8(0);
				for (const edge of box.borderColor) for (const channel of edge) this.binary.u8(channel);
				this.binary.u8(alpha(box.opacity));
				// Two bits per side, top first.
				this.binary.u8(
					box.borderStyle.reduce((bits, style, side) => bits | (style << (side * 2)), 0),
				);
				const { radius } = box;
				this.binary.u8(radius ? 1 : 0);
				if (radius) {
					this.binary.u8(radius.percent);
					for (const value of radius.values) this.binary.f32(value);
				}
				const size = box.size;
				this.binary.u8((size?.borderBox ? 1 : 0) | ((size?.autoMargin ?? 0) << 1));
				this.size(size?.width);
				this.size(size?.minWidth);
				this.size(size?.maxWidth);
				this.height(size?.height);
				this.height(size?.minHeight);
				this.height(size?.maxHeight);
			}),
		);
	}
	/** Kind 0 is `auto`. */
	private size(value: Dimension | undefined) {
		this.binary.u8(value?.kind ?? 0);
		if (value) this.binary.f32(value.value);
	}

	private height(value: number | undefined) {
		this.size(value === undefined ? undefined : { kind: 1, value });
	}

	boxEnd(through = false) {
		this.close(through, opcode.boxEnd);
	}

	end() {
		this.binary.record(opcode.end);
		this.binary.flush();
	}

	private spacing(run: Run) {
		if (run.letterSpacing) this.binary.f32(run.letterSpacing);
		if (run.wordSpacing) this.binary.f32(run.wordSpacing);
	}

	private open(through: boolean, write: () => void) {
		this.body(!through);
		if (through) this.starts.push(write);
		else write();
	}

	private close(through: boolean, code: number) {
		if (!through) this.body();
		// The start is still held, so the box is empty and its margins collapse through it.
		else if (this.starts.pop()) return;
		this.binary.record(code);
	}

	/** Every body record first writes what is held, unless it is held itself. */
	private body(flush = true) {
		if (flush) this.flush();
	}
}
