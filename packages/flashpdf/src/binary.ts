/** The adapter contract both backends implement: a fixed window plus a push. */
export interface InputWindow {
	input_ptr(): number;
	input_capacity(): number;
	push(length: number): void;
	finish(): Uint8Array;
}

export class Binary {
	private readonly bytes = new Uint8Array(65541);
	private readonly view = new DataView(this.bytes.buffer);
	private readonly encoder = new TextEncoder();
	private readonly inputPtr: number;
	private readonly inputCapacity: number;
	private input: Uint8Array;
	private offset = 0;
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
	f32(value: number) {
		this.reserve(4);
		this.view.setFloat32(this.offset, value, true);
		this.offset += 4;
	}
	text(value: string) {
		this.u32(0);
		const start = this.offset;
		const { read, written } = this.encoder.encodeInto(value, this.bytes.subarray(start));
		if (read !== value.length) throw new Error("record too large");
		this.view.setUint32(start - 4, written, true);
		this.offset += written;
	}
	header(width: number, height: number, margin: number) {
		for (const byte of [70, 80, 68, 70]) this.u8(byte);
		this.u16(2);
		this.f32(width);
		this.f32(height);
		this.f32(margin);
		this.flush();
	}
	record(opcode: number, payload: () => void = () => {}) {
		this.u8(opcode);
		this.u32(0);
		payload();
		this.view.setUint32(1, this.offset - 5, true);
		this.flush();
	}
	private reserve(length: number) {
		if (this.offset + length > this.bytes.length) throw new Error("record too large");
	}
	private flush() {
		for (let offset = 0; offset < this.offset; offset += this.inputCapacity) {
			if (this.input.buffer !== this.memory.buffer)
				this.input = new Uint8Array(this.memory.buffer, this.inputPtr, this.inputCapacity);
			const chunk = this.bytes.subarray(offset, Math.min(offset + this.inputCapacity, this.offset));
			this.input.set(chunk);
			this.renderer.push(chunk.length);
		}
		this.offset = 0;
	}
}
