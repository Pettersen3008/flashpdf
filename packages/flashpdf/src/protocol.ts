import { asError, Binary } from "./binary.js";
import type { FontSlots } from "./fonts.js";
import type { PageOptions } from "./page.js";
import type { Drawing, Paint } from "./svg.js";
import { tags, type ElementNode, type Node } from "./tree.js";

const opcode = {
	metadata: 14,
	pdfa: 20,
	attachment: 21,
	family: 26,
	stylesheet: 27,
	setup: 28,
	block: 29,
	open: 30,
	text: 31,
	close: 32,
	drawing: 33,
	strictStylesheet: 34,
	html: 35,
	namedImage: 36,
	tw: 37,
	end: 255,
} as const;
/** Open record flags: which attributes follow the classes. */
const flag = {
	id: 1,
	alt: 2,
	href: 4,
	title: 8,
	noBookmark: 16,
	start: 32,
	reversed: 64,
	value: 128,
	colSpan: 256,
	rowSpan: 512,
	raster: 1024,
	styleText: 2048,
	styleObject: 4096,
	ltr: 8192,
	rtl: 16384,
	auto: 32768,
} as const;
const TAGS = new Map<string, number>(tags.map((tag, index) => [tag, index]));
/** The core's cap; checked here because `stage_image` copies the bytes into WASM memory. */
const MAX_IMAGE_BYTES = 64 * 1024 * 1024;
/** Payload bytes per record of a value split across records, after its continuation byte. */
const CHUNK = 65535;
/** Characters that fit one record at three UTF-8 bytes each. */
const SHORT = Math.floor(CHUNK / 3);

/** Writes the record stream: document records, then the element tree the core compiles. */
export class ProtocolWriter {
	/** Handle per staged byte array, so a logo reused on every row embeds once. */
	private readonly images = new Map<Uint8Array, number>();
	private imageBytes = 0;
	private readonly encoder = new TextEncoder();

	constructor(
		private readonly binary: Binary,
		private readonly assets: { stage_image(bytes: Uint8Array): number },
	) {}

	header() {
		this.binary.header();
	}

	families(families: ReadonlyMap<string, FontSlots>) {
		for (const [family, slots] of families) {
			if (family === "Helvetica") continue;
			this.binary.record(opcode.family, () => {
				this.binary.text(family);
				for (const slot of [slots.regular, slots.bold, slots.italic, slots.boldItalic])
					this.binary.u8(slot ?? 255);
			});
		}
	}

	/** A strict sheet gets the core's full check, as `stylesheet()` promises. */
	stylesheet(source: string, strict: boolean) {
		this.long(strict ? opcode.strictStylesheet : opcode.stylesheet, source);
	}

	/** Kind 0 is the body, 1 to 4 the furniture, as in `block`. */
	html(kind: number, source: string) {
		const bytes = this.encoder.encode(source);
		const value = new Uint8Array(bytes.length + 1);
		value[0] = kind;
		value.set(bytes, 1);
		this.long(opcode.html, value);
	}

	/** An `images` entry: its name, then 0 and a staged raster handle or 1 and a drawing. */
	image(name: string, picture: { src?: Uint8Array | undefined; drawing?: Drawing | undefined }) {
		const label = this.encoder.encode(name);
		const shape = picture.drawing && drawing(picture.drawing);
		const value = new Uint8Array(5 + label.length + (shape ? shape.length : 2));
		const view = new DataView(value.buffer);
		view.setUint32(0, label.length, true);
		value.set(label, 4);
		if (shape) {
			value[4 + label.length] = 1;
			value.set(shape, 5 + label.length);
		} else view.setUint16(5 + label.length, this.stage(picture.src!), true);
		this.long(opcode.namedImage, value);
	}

	setup(outline: boolean, page: PageOptions) {
		this.binary.record(opcode.setup, () => {
			this.binary.u8(outline ? 1 : 0);
			const { format, margin } = page;
			if (format === undefined) this.binary.u8(0);
			else if (Array.isArray(format)) {
				this.binary.u8(2);
				for (const side of format) this.value(side);
			} else {
				this.binary.u8(1);
				this.value(format);
			}
			this.value(page.orientation);
			if (margin === undefined) this.binary.u8(0);
			else if (Array.isArray(margin)) {
				this.binary.u8(2);
				for (const side of margin) this.value(side);
			} else {
				this.binary.u8(1);
				this.value(margin);
			}
		});
	}

	metadata(value: Record<string, unknown>, tagged: boolean) {
		this.binary.record(opcode.metadata, () => {
			this.binary.u8(tagged ? 1 : 0);
			for (const key of ["title", "author", "subject", "keywords", "language"])
				this.binary.text((value[key] as string | undefined) ?? "");
		});
	}

	/** Profile 0 is plain PDF/A-3b, 7 PDF/A-2b, 8 PDF/A-2u; 1 to 6 are PDF/A-3b with the Factur-X XML registered as `xml`. */
	pdfa(profile: number, xml?: number) {
		this.binary.record(opcode.pdfa, () => {
			this.binary.u8(profile);
			if (xml !== undefined) this.binary.u16(xml);
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

	/** Kind 0 is the body, 1 to 4 the header, footer, first-page header, and first-page footer. */
	block(kind: number, nodes: readonly Node[]) {
		this.binary.record(opcode.block, () => this.binary.u8(kind));
		this.nodes(nodes);
	}

	/** Pushes what is queued, so the core reports a bad option before the tree is resolved. */
	flush() {
		this.binary.flush();
	}

	end() {
		this.binary.record(opcode.end);
		this.binary.flush();
	}

	private nodes(nodes: readonly Node[]) {
		for (const node of nodes)
			if (node.kind === "text") this.long(opcode.text, node.value);
			else this.element(node);
	}

	private element(node: ElementNode) {
		const raster = node.src && !node.drawing ? this.stage(node.src) : undefined;
		const style = node.style;
		const title = typeof node.bookmark === "string" ? node.bookmark : undefined;
		const flags =
			(node.id !== undefined ? flag.id : 0) |
			(node.alt !== undefined ? flag.alt : 0) |
			(node.href !== undefined ? flag.href : 0) |
			(title !== undefined ? flag.title : 0) |
			(node.bookmark === false ? flag.noBookmark : 0) |
			(node.start !== undefined ? flag.start : 0) |
			(node.reversed ? flag.reversed : 0) |
			(node.value !== undefined ? flag.value : 0) |
			(node.colSpan !== undefined ? flag.colSpan : 0) |
			(node.rowSpan !== undefined ? flag.rowSpan : 0) |
			(raster !== undefined ? flag.raster : 0) |
			(typeof style === "string" ? flag.styleText : 0) |
			(typeof style === "object" ? flag.styleObject : 0) |
			(node.dir !== undefined ? flag[node.dir] : 0);
		const binary = this.binary;
		binary.record(opcode.open, () => {
			binary.u8(TAGS.get(node.tag)!);
			binary.u16(flags);
			if (node.id !== undefined) binary.text(node.id);
			binary.u16(node.classes.length);
			for (const name of node.classes) binary.text(name);
			if (typeof style === "string") binary.text(style);
			else if (typeof style === "object") {
				const entries = Object.entries(style).filter(([, value]) => value !== undefined);
				binary.u16(entries.length);
				for (const [key, value] of entries) {
					binary.text(key);
					this.value(value);
				}
			}
			if (node.alt !== undefined) binary.text(node.alt);
			if (node.href !== undefined) binary.text(node.href);
			if (title !== undefined) binary.text(title);
			if (node.start !== undefined) binary.i32(node.start);
			if (node.value !== undefined) binary.i32(node.value);
			if (node.colSpan !== undefined) binary.u16(node.colSpan);
			if (node.rowSpan !== undefined) binary.u16(node.rowSpan);
			if (raster !== undefined) binary.u16(raster);
		});
		if (node.tw) binary.record(opcode.tw, () => binary.text(node.tw!));
		if (node.drawing) this.long(opcode.drawing, drawing(node.drawing));
		this.nodes(node.children);
		binary.record(opcode.close);
	}

	private stage(src: Uint8Array): number {
		let handle = this.images.get(src);
		if (handle === undefined) {
			if ((this.imageBytes += src.byteLength) > MAX_IMAGE_BYTES)
				throw new Error("images exceed 64 MiB per render");
			try {
				handle = this.assets.stage_image(src);
			} catch (error) {
				throw asError(error);
			}
			this.images.set(src, handle);
		}
		return handle;
	}

	/** A string or number as the host passed it; the core validates and reports it. Anything
	 *  else only reaches here under a style key the core rejects by name. */
	private value(value: unknown) {
		if (value === undefined) this.binary.u8(0);
		else if (typeof value === "number") {
			this.binary.u8(2);
			this.binary.f64(value);
		} else {
			this.binary.u8(1);
			this.binary.text(String(value));
		}
	}

	/** A text or byte value that may span records: each starts with 1 while more follow. */
	private long(code: number, value: string | Uint8Array) {
		const binary = this.binary;
		if (typeof value === "string" && value.length <= SHORT)
			return binary.record(code, () => {
				binary.u8(0);
				binary.utf8(value);
			});
		const bytes = typeof value === "string" ? this.encoder.encode(value) : value;
		for (let at = 0; ; at += CHUNK) {
			const more = at + CHUNK < bytes.length;
			binary.record(code, () => {
				binary.u8(more ? 1 : 0);
				binary.bytes(bytes.subarray(at, at + CHUNK));
			});
			if (!more) return;
		}
	}
}

/** svg.ts's drawing for the core: f64 scalars and f32 path coordinates. Each shape's own
 *  opacity is folded into its fill and stroke opacity. */
function drawing(value: Drawing): Uint8Array {
	// An upper bound: a currentColor paint takes 1 byte, not 4.
	let size = 48 + 1 + (value.align ? 16 : 0) + 1 + 4;
	for (const shape of value.shapes)
		size +=
			48 +
			(shape.fill ? 4 : 1) +
			1 +
			8 +
			(shape.stroke ? 4 : 1) +
			8 +
			2 +
			8 +
			1 +
			8 * shape.dashes.length +
			16 +
			4 +
			shape.ops.length +
			4 * shape.points.length;
	const bytes = new Uint8Array(size);
	const view = new DataView(bytes.buffer);
	let offset = 0;
	const u8 = (byte: number) => view.setUint8(offset++, byte);
	const f64 = (number: number) => {
		view.setFloat64(offset, number, true);
		offset += 8;
	};
	const paint = (paint: Paint) => {
		if (!paint) return u8(0);
		if (paint === "currentColor") return u8(2);
		u8(1);
		for (const channel of paint) u8(channel);
	};
	for (const number of [value.width, value.height, ...value.viewBox]) f64(number);
	u8(value.align ? 1 : 0);
	for (const factor of value.align ?? []) f64(factor);
	u8(value.slice ? 1 : 0);
	view.setUint32(offset, value.shapes.length, true);
	offset += 4;
	for (const shape of value.shapes) {
		for (const number of shape.matrix) f64(number);
		paint(shape.fill);
		u8(shape.evenOdd ? 1 : 0);
		f64(shape.fillOpacity * shape.opacity);
		paint(shape.stroke);
		f64(shape.strokeWidth);
		u8(shape.cap);
		u8(shape.join);
		f64(shape.miter);
		u8(shape.dashes.length);
		for (const dash of shape.dashes) f64(dash);
		f64(shape.dashOffset);
		f64(shape.strokeOpacity * shape.opacity);
		view.setUint32(offset, shape.ops.length, true);
		offset += 4;
		bytes.set(shape.ops, offset);
		offset += shape.ops.length;
		for (const point of shape.points) {
			view.setFloat32(offset, point, true);
			offset += 4;
		}
	}
	return bytes.subarray(0, offset);
}
