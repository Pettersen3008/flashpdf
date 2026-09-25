import init from "../wasm/flashpdf_wasm.js";

type FsPromises = typeof import("node:fs/promises");

/** Node cannot fetch a file: URL. getBuiltinModule keeps the specifier static; the
 *  dynamic import only serves runtimes without it and stays opaque to bundlers. */
async function readFile(url: URL): Promise<Uint8Array> {
	const specifier = "node:fs/promises";
	const fs =
		(globalThis.process?.getBuiltinModule?.(specifier) as FsPromises | undefined) ??
		((await import(/* webpackIgnore: true */ /* @vite-ignore */ specifier)) as FsPromises);
	return fs.readFile(url);
}

let initialization: ReturnType<typeof init> | undefined;
export function loadWasm(): ReturnType<typeof init> {
	return (initialization ??= (async () => {
		const url = new URL("../wasm/flashpdf_wasm_bg.wasm", import.meta.url);
		return init({ module_or_path: url.protocol === "file:" ? await readFile(url) : url });
	})());
}
