import type { Backend, Engine } from "./binary.js";
import { loadBackend as loadWasm } from "./wasm.js";

type NativeEngine = Omit<Engine, "push"> & { push_bytes(input: Uint8Array): void };
export type Addon = { INPUT_CAPACITY: number; PdfRenderer: new () => NativeEngine };
type NodeModule = typeof import("node:module");

/** napi-rs's per-platform package names; on Linux the wrong libc fails to load and the next one tries. */
function candidates(): string[] {
	const base = `@pettersen3008/flashpdf-${process.platform}-${process.arch}`;
	return process.platform === "linux" ? [`${base}-gnu`, `${base}-musl`] : [base];
}

/** getBuiltinModule keeps `require` opaque to bundlers, as in wasm.ts. */
function loadAddon(): Addon | undefined {
	const module = globalThis.process?.getBuiltinModule?.("node:module") as NodeModule | undefined;
	if (!module) return undefined;
	const require = module.createRequire(import.meta.url);
	for (const id of candidates()) {
		try {
			return require(id) as Addon;
		} catch {
			// No prebuilt addon for this platform: WASM renders the same bytes.
		}
	}
	return undefined;
}

let native: Backend | null | undefined;
export function loadBackend(): Promise<Backend> {
	if (native === undefined) {
		const addon = loadAddon();
		native = addon ? nativeBackend(addon) : null;
	}
	return native ? Promise.resolve(native) : loadWasm();
}

/** Every engine shares one window: a render writes and pushes synchronously from create to finish. */
export function nativeBackend(addon: Addon): Backend {
	const window = new Uint8Array(addon.INPUT_CAPACITY);
	return {
		memory: window,
		create() {
			const engine = new addon.PdfRenderer();
			// Lent per call; see crates/flashpdf-napi/src/lib.rs.
			return Object.assign(engine, {
				push: (length: number) => engine.push_bytes(window.subarray(0, length)),
			});
		},
	};
}
