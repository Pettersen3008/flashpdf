/** The adapter contract both backends implement: a fixed window plus a push. */
export interface InputWindow {
	input_ptr(): number;
	input_capacity(): number;
	push(length: number): void;
	finish(): Uint8Array;
}

/** One render's engine; the WASM glue's `PdfRenderer` and the N-API addon both fit. */
export interface Engine extends InputWindow {
	add_font(bytes: Uint8Array): number;
	stage_image(bytes: Uint8Array): number;
	add_attachment(bytes: Uint8Array): number;
	free(): void;
}

/** What `#backend` loads: a factory for engines and the memory their input windows live in. */
export type Backend = { memory: { buffer: ArrayBufferLike }; create(): Engine };

/** The largest record: opcode, u32 length, and a 64 KiB payload. */
const RECORD = 65541;

/** wasm-bindgen throws a `JsValue` string. */
export function asError(error: unknown): Error {
	return error instanceof Error ? error : new Error(String(error));
}

/** Queues records and pushes them up to 64 KiB at a time, because each push is a WASM call. */
export class Binary {
	// Flushed once it holds a whole record's worth, so the next record always fits.
	private readonly buffer = new Uint8Array(2 * RECORD);
	private readonly view = new DataView(this.buffer.buffer);
	private readonly encoder = new TextEncoder();
	private readonly inputPtr: number;
	private readonly inputCapacity: number;
	private input: Uint8Array;
	private offset = 0;
	/** Where the record being written starts. */
	private start = 0;
	constructor(
		private readonly renderer: InputWindow,
		private readonly memory: { buffer: ArrayBufferLike },
	) {
		this.inputPtr = renderer.input_ptr();
		this.inputCapacity = renderer.input_capacity();
		this.input = new Uint8Array(memory.buffer, this.inputPtr, this.inputCapacity);
	}
	u8(value: number) {
		this.reserve(1);
		this.view.setUint8(this.offset++, value);
	}
	u16(value: number) {
		this.reserve(2);
		this.view.setUint16(this.offset, value, true);
		this.offset += 2;
	}
	u32(value: number) {
		this.reserve(4);
		this.view.setUint32(this.offset, value, true);
		this.offset += 4;
	}
	i32(value: number) {
		this.reserve(4);
		this.view.setInt32(this.offset, value, true);
		this.offset += 4;
	}
	f32(value: number) {
		this.reserve(4);
		this.view.setFloat32(this.offset, value, true);
		this.offset += 4;
	}
	f64(value: number) {
		this.reserve(8);
		this.view.setFloat64(this.offset, value, true);
		this.offset += 8;
	}
	bytes(value: Uint8Array) {
		this.reserve(value.length);
		this.buffer.set(value, this.offset);
		this.offset += value.length;
	}
	text(value: string) {
		this.u32(0);
		const start = this.offset;
		this.view.setUint32(start - 4, this.utf8(value), true);
	}
	/** UTF-8 without a length; TextEncoder turns a lone surrogate into U+FFFD. */
	utf8(value: string): number {
		const { read, written } = this.encoder.encodeInto(
			value,
			this.buffer.subarray(this.offset, this.start + RECORD),
		);
		if (read !== value.length) throw new Error("record too large");
		this.offset += written;
		return written;
	}
	header() {
		for (const byte of [70, 80, 68, 70]) this.u8(byte);
		this.u16(26);
		this.flush();
	}
	record(opcode: number, payload: () => void = () => {}) {
		this.u8(opcode);
		this.u32(0);
		payload();
		this.view.setUint32(this.start + 1, this.offset - this.start - 5, true);
		if (this.offset >= RECORD) this.flush();
		this.start = this.offset;
	}
	flush() {
		for (let offset = 0; offset < this.offset; offset += this.inputCapacity) {
			if (this.input.buffer !== this.memory.buffer)
				this.input = new Uint8Array(this.memory.buffer, this.inputPtr, this.inputCapacity);
			const chunk = this.buffer.subarray(
				offset,
				Math.min(offset + this.inputCapacity, this.offset),
			);
			this.input.set(chunk);
			try {
				this.renderer.push(chunk.length);
			} catch (error) {
				throw asError(error);
			}
		}
		this.offset = this.start = 0;
	}
	private reserve(length: number) {
		if (this.offset + length > this.start + RECORD) throw new Error("record too large");
	}
}
