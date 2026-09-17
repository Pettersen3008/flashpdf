import init from "../wasm/flashpdf_wasm.js";

let initialization: ReturnType<typeof init> | undefined;
export function loadWasm(): ReturnType<typeof init> {
	if (!initialization) {
		const module_or_path = new URL("../wasm/flashpdf_wasm_bg.wasm", import.meta.url);
		initialization = init({ module_or_path });
	}
	return initialization;
}
