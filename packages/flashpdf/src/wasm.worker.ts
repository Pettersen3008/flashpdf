import { initSync, PdfRenderer } from "../wasm/flashpdf_wasm.js";
// @ts-expect-error Wrangler loads .wasm imports as WebAssembly.Module.
import module from "../wasm/flashpdf_wasm_bg.wasm";
import type { Backend } from "./binary.js";

let initialization: Backend | undefined;
export function loadBackend(): Promise<Backend> {
	initialization ??= { memory: initSync({ module }).memory, create: () => new PdfRenderer() };
	return Promise.resolve(initialization);
}
