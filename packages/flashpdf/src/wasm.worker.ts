import { initSync } from "../wasm/flashpdf_wasm.js";
// @ts-expect-error Wrangler loads .wasm imports as WebAssembly.Module.
import module from "../wasm/flashpdf_wasm_bg.wasm";

let initialization: ReturnType<typeof initSync> | undefined;
export function loadWasm(): Promise<ReturnType<typeof initSync>> {
	return Promise.resolve((initialization ??= initSync({ module })));
}
