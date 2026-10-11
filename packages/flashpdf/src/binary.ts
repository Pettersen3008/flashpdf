/** The adapter contract both backends implement: a fixed window plus a push. */
export interface InputWindow {
	input_ptr(): number;
	input_capacity(): number;
	push(length: number): void;
	/** Records decoded so far; after a failed push, the index of the failing record. */
	records(): number;
	finish(): Uint8Array;
}

/** The largest record: opcode, u32 length, and a 64 KiB payload. */
const RECORD = 65541;

/** wasm-bindgen throws a `JsValue` string; an `Error` lets `locate()` append the element path. */
export function asError(error: unknown): Error {
	return error instanceof Error ? error : new Error(String(error));
}

/** Queues records and pushes them up to 64 KiB at a time, because each push is a WASM call. */
export class Binary {
	// Flushed once it holds a whole record's worth, so the next record always fits.
	private readonly bytes = new Uint8Array(2 * RECORD);
	private readonly view = new DataView(this.bytes.buffer);
	private readonly encoder = new TextEncoder();
	private readonly inputPtr: number;
	private readonly inputCapacity: number;
	private input: Uint8Array;
	private offset = 0;
	/** Where the record being written starts. */
	private start = 0;
	private written = 0;
	private failed: number | undefined;
	/** `replayAt` pushes that record alone, so the engine's error surfaces at the call that wrote it. */
	constructor(
		private readonly renderer: InputWindow,
		private readonly memory: { buffer: ArrayBufferLike },
		private readonly replayAt = Infinity,
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
	f32(value: number) {
		this.reserve(4);
		this.view.setFloat32(this.offset, value, true);
		this.offset += 4;
	}
	text(value: string) {
		this.u32(0);
		const start = this.offset;
		const { read, written } = this.encoder.encodeInto(
			value,
			this.bytes.subarray(start, this.start + RECORD),
		);
		if (read !== value.length) throw new Error("record too large");
		this.view.setUint32(start - 4, written, true);
		this.offset += written;
	}
	header(width: number, height: number, margin: number) {
		for (const byte of [70, 80, 68, 70]) this.u8(byte);
		this.u16(17);
		this.f32(width);
		this.f32(height);
		this.f32(margin);
		this.flush();
	}
	record(opcode: number, payload: () => void = () => {}) {
		if (this.written === this.replayAt) this.flush();
		this.u8(opcode);
		this.u32(0);
		payload();
		this.view.setUint32(this.start + 1, this.offset - this.start - 5, true);
		if (++this.written > this.replayAt || this.offset >= RECORD) this.flush();
		this.start = this.offset;
	}
	flush() {
		for (let offset = 0; offset < this.offset; offset += this.inputCapacity) {
			if (this.input.buffer !== this.memory.buffer)
				this.input = new Uint8Array(this.memory.buffer, this.inputPtr, this.inputCapacity);
			const chunk = this.bytes.subarray(offset, Math.min(offset + this.inputCapacity, this.offset));
			this.input.set(chunk);
			try {
				this.renderer.push(chunk.length);
			} catch (error) {
				this.failed = this.renderer.records();
				throw asError(error);
			}
		}
		this.offset = this.start = 0;
	}
	/** After an error, pushes the records finished before it and returns the index of the
	 *  first one the engine rejected: that error came first, so the caller replays to it. */
	failure(): number | undefined {
		if (this.failed === undefined) {
			this.offset = this.start;
			try {
				this.flush();
			} catch {
				// Recorded in `failed`.
			}
		}
		return this.failed;
	}
	private reserve(length: number) {
		if (this.offset + length > this.start + RECORD) throw new Error("record too large");
	}
}
