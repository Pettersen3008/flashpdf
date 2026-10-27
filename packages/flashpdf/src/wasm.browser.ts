import init, { PdfRenderer } from "../wasm/flashpdf_wasm.js";
import type { Backend } from "./binary.js";

let initialization: Promise<Backend> | undefined;
export function loadBackend(): Promise<Backend> {
	if (!initialization) {
		const module_or_path = new URL("../wasm/flashpdf_wasm_bg.wasm", import.meta.url);
		initialization = init({ module_or_path }).then(
			({ memory }) => ({ memory, create: () => new PdfRenderer() }),
			(error: unknown) => {
				// A failed fetch must not poison every later render.
				initialization = undefined;
				throw error;
			},
		);
	}
	return initialization;
}
