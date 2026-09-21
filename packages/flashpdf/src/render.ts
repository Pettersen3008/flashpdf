import { PdfRenderer } from "../wasm/flashpdf_wasm.js";
import { number, props } from "./assert.js";
import { Binary } from "./binary.js";
import { compile } from "./compile.js";
import { resolveStyles } from "./css.js";
import type { Element } from "./element.js";
import { registerFonts } from "./fonts.js";
import { ProtocolWriter } from "./protocol.js";
import { resolveTree } from "./tree.js";

export type EmbeddedFont = { family: string; regular: Uint8Array; bold?: Uint8Array };
export type RenderOptions = {
	pageFormat?: "A4" | "Letter";
	margin?: number;
	stylesheets?: readonly string[];
	fonts?: readonly EmbeddedFont[] | undefined;
	footer?: Element | undefined;
};

type LoadWasm = () => Promise<{ memory: { buffer: ArrayBufferLike } }>;

export function createRenderer(loadWasm: LoadWasm) {
	return async function render(
		document: Element | Iterable<Element>,
		options?: RenderOptions,
	): Promise<Uint8Array> {
		const tree = await resolveTree(document);
		if (tree.some((node) => node.kind !== "element")) throw new Error("expected element or props");
		const p = props(options ?? {}, ["pageFormat", "margin", "stylesheets", "fonts", "footer"]);
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
				(!Array.isArray(p.stylesheets) || p.stylesheets.some((sheet) => typeof sheet !== "string"))
			)
				throw new Error("invalid stylesheets");
			if (p.footer !== undefined) {
				const footer = await resolveTree(p.footer);
				if (footer.length !== 1 || footer[0]?.kind !== "element")
					throw new Error("footer must resolve to one element");
				writer.footerStart();
				compile(
					resolveStyles(footer, p.stylesheets as readonly string[] | undefined),
					writer,
					fonts,
					undefined,
					0,
					true,
				);
				writer.footerEnd();
			}
			compile(resolveStyles(tree, p.stylesheets as readonly string[] | undefined), writer, fonts);
			writer.end();
			consumed = true;
			return renderer.finish();
		} finally {
			if (!consumed) renderer.free();
		}
	};
}
