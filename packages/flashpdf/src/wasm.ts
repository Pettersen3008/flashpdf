import init from "../wasm/flashpdf_wasm.js";

let initialization: ReturnType<typeof init> | undefined;
export function loadWasm(): ReturnType<typeof init> {
	if (!initialization) {
		const url = new URL("../wasm/flashpdf_wasm_bg.wasm", import.meta.url);
		initialization = (async () => {
			// Node cannot fetch a file: URL. The specifier stays non-literal so a
			// browser bundler never tries to resolve node:fs/promises.
			const nodeFs = "node:fs/promises";
			const module_or_path =
				url.protocol === "file:"
					? await (await import(/* @vite-ignore */ nodeFs)).readFile(url)
					: url;
			return init({ module_or_path });
		})();
	}
	return initialization;
}
