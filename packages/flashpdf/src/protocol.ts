import { asError, Binary } from "./binary.js";
import type { Box, Transform, WhiteSpace } from "./style.js";

export type Column = { kind: 0 | 1 | 2; value: number };
/** An image dimension: 1 is points, 2 is percent of the container width. */
export type Dimension = { kind: 1 | 2; value: number };
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
	pdfa: 20,
	attachment: 21,
	end: 255,
} as const;

export class ProtocolWriter {
	private repeating: "header" | "footer" | undefined;
	private repeatWritten = false;
	/** Slot per registered byte array, so a logo reused on every row embeds once. */
	private readonly images = new Map<Uint8Array, number>();
	private held = 0;
	/** Box and stack starts waiting for content, so a held margin lands outside them. */
	private readonly starts: (() => void)[] = [];

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

	paragraph(runs: readonly Run[], align: string) {
		const alignment = ["left", "center", "right", "justify"].indexOf(align);
		if (this.repeating) {
			const [only] = runs;
			if (this.repeatWritten || runs.length !== 1 || only!.break)
				throw new Error(`${this.repeating} must be a single text line`);
			if (
				only!.underline ||
				only!.lineThrough ||
				only!.link !== undefined ||
				only!.rise ||
				only!.background ||
				only!.marker
			)
				throw new Error(
					`${this.repeating} text cannot be underlined, linked, raised, or highlighted`,
				);
			this.repeatWritten = true;
			this.binary.record(opcode[this.repeating], () => {
				this.binary.f32(only!.size);
				this.binary.u8(alignment);
				for (const channel of only!.color) this.binary.u8(channel);
				this.binary.u8(only!.font);
				this.binary.u16(typography(only!));
				this.spacing(only!);
				this.binary.text(only!.text);
			});
			return;
		}
		this.body();
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
					// Bit 0 hard break, 1 underline, 2 line-through, 3 a u16 link index follows,
					// 4 an f32 line height follows, 5 to 7, 9, and 11 to 14 typography, 8 a decoration
					// colour follows, 10 a background colour, 11 an f32 baseline shift, 12 no wrap,
					// and 13 and 14 a marker.
					this.binary.u16(
						(run.break ? 1 : 0) |
							(run.underline ? 2 : 0) |
							(run.lineThrough ? 4 : 0) |
							(run.link === undefined ? 0 : 8) |
							(run.lineHeight ? 16 : 0) |
							typography(run) |
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
	tableStart(headerRows: number, footerRows: number, width: Column) {
		this.body();
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
		this.body();
		this.binary.record(opcode.tableEnd);
	}

	/** CSS drops the margin before a forced break and keeps the one after. */
	pageBreak() {
		this.held = 0;
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
			}),
		);
	}

	boxEnd(through = false) {
		this.close(through, opcode.boxEnd);
	}

	end() {
		this.binary.record(opcode.end);
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
		if (this.repeating) throw new Error(`${this.repeating} must be a single text line`);
		if (flush) this.flush();
	}
}
