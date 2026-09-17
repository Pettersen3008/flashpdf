import { PdfRenderer } from "../wasm/flashpdf_wasm.js";
import { number, object, props } from "./assert.js";
import { Binary } from "./binary.js";
import { resolveStyles } from "./css.js";
import type { Element } from "./element.js";
import { registerFonts } from "./fonts.js";
import { lower } from "./lower.js";
import { ProtocolWriter } from "./protocol.js";
import { children, reactTree } from "./tree.js";

export type EmbeddedFont = { family: string; regular: Uint8Array; bold?: Uint8Array };
export type RenderOptions = {
	pageFormat?: "A4" | "Letter";
	margin?: number;
	stylesheets?: readonly string[];
	fonts?: readonly EmbeddedFont[] | undefined;
};

type LoadWasm = () => Promise<{ memory: { buffer: ArrayBufferLike } }>;

export function createRenderer(loadWasm: LoadWasm) {
	return async function render(
		document: Element | Iterable<Element>,
		options?: RenderOptions,
	): Promise<Uint8Array> {
		const tree = await reactTree(document);
		for (const child of children(tree)) object(child);
		const p = props(options ?? {}, ["pageFormat", "margin", "stylesheets", "fonts"]);
		const format = p.pageFormat === undefined ? "A4" : p.pageFormat;
		if (format !== "A4" && format !== "Letter") throw new Error("invalid page format");
		const margin = number(p.margin === undefined ? 36 : p.margin);
		const width = format === "A4" ? 595 : 612;
		const height = format === "A4" ? 842 : 792;
		if (margin * 2 >= Math.min(width, height)) throw new Error("invalid margin");
		const wasm = await loadWasm();
		const renderer = new PdfRenderer();
		let consumed = false;
		try {
			const fonts = registerFonts(renderer, p.fonts);
			const writer = new ProtocolWriter(new Binary(renderer, wasm.memory));
			writer.header(width, height, margin);
			if (
				p.stylesheets !== undefined &&
				(!Array.isArray(p.stylesheets) ||
					p.stylesheets.some((sheet) => typeof sheet !== "string"))
			)
				throw new Error("invalid stylesheets");
			lower(resolveStyles(tree, p.stylesheets as readonly string[] | undefined), writer, fonts);
			writer.end();
			consumed = true;
			return renderer.finish();
		} finally {
			if (!consumed) renderer.free();
		}
	};
}
