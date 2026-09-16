import { createRequire } from "node:module";
import type { InputWindow } from "./binary.js";

interface Native {
  addFont(bytes: Buffer): number;
  inputCapacity(): number;
  push(chunk: Buffer): void;
  finish(): Uint8Array;
}

const addon = createRequire(import.meta.url)("../../../dist/napi/flashpdf.node") as {
  PdfRenderer: new () => Native;
};

/**
 * Presents the N-API addon as the window `Binary` already writes into. Node has
 * no shared linear memory, so the window is a JS buffer the addon borrows and
 * copies during each synchronous push.
 */
export function createNativeRenderer(): {
  renderer: InputWindow & { add_font(bytes: Buffer): number };
  memory: { buffer: ArrayBufferLike };
} {
  const native = new addon.PdfRenderer();
  const input = Buffer.from(new ArrayBuffer(native.inputCapacity()));
  return {
    renderer: {
      add_font: (bytes: Buffer) => native.addFont(bytes),
      input_ptr: () => 0,
      input_capacity: () => input.length,
      push: (length: number) => native.push(input.subarray(0, length)),
      finish: () => native.finish(),
    },
    memory: { buffer: input.buffer },
  };
}
