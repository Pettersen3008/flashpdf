import type { ReactElement } from "react";

import { PdfRenderer } from "../wasm/flashpdf_wasm.js";
import { object, props, number } from "./assert.js";
import { Binary } from "./binary.js";
import { resolveStyles } from "./css.js";
import { registerFonts } from "./fonts.js";
import { lower } from "./lower.js";
import { reactTree, children } from "./tree.js";
import { loadWasm } from "./wasm.js";

export type { Style } from "./element.js";
export { stylesheet } from "./css.js";

export type EmbeddedFont = { family: string; regular: Uint8Array; bold?: Uint8Array };
export type RenderOptions = {
	pageFormat?: "A4" | "Letter";
	margin?: number;
	stylesheets?: readonly string[];
	fonts?: readonly EmbeddedFont[] | undefined;
};

export async function render(
	document: ReactElement | Iterable<ReactElement>,
	options?: RenderOptions,
): Promise<Uint8Array> {
	const tree = await reactTree(document);
	// A root fragment flattens to an array, so validate its members, not the array.
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
		const binary = new Binary(renderer, wasm.memory);
		binary.header(width, height, margin);
		if (
			p.stylesheets !== undefined &&
			(!Array.isArray(p.stylesheets) || p.stylesheets.some((sheet) => typeof sheet !== "string"))
		)
			throw new Error("invalid stylesheets");
		await lower(resolveStyles(tree, p.stylesheets as readonly string[] | undefined), binary, fonts);
		binary.record(255);
		consumed = true;
		return renderer.finish();
	} finally {
		if (!consumed) renderer.free();
	}
}
