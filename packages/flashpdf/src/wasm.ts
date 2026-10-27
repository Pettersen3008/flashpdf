import init, { PdfRenderer } from "../wasm/flashpdf_wasm.js";
import type { Backend } from "./binary.js";

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

let initialization: Promise<Backend> | undefined;
export function loadBackend(): Promise<Backend> {
	return (initialization ??= (async () => {
		const url = new URL("../wasm/flashpdf_wasm_bg.wasm", import.meta.url);
		const { memory } = await init({
			module_or_path: url.protocol === "file:" ? await readFile(url) : url,
		});
		return { memory, create: () => new PdfRenderer() };
	})().catch((error: unknown) => {
		// A failed load must not poison every later render.
		initialization = undefined;
		throw error;
	}));
}
