import { PdfRenderer } from "../wasm/flashpdf_wasm.js";
import { number, props } from "./assert.js";
import { asError, Binary } from "./binary.js";
import { compile } from "./compile.js";
import { resolveStyles } from "./css.js";
import type { Element } from "./element.js";
import { registerFonts } from "./fonts.js";
import { ProtocolWriter } from "./protocol.js";
import { resolveTree } from "./tree.js";

export type EmbeddedFont = { family: string; regular: Uint8Array; bold?: Uint8Array | undefined };
export type PdfMetadata = {
	title?: string | undefined;
	author?: string | undefined;
	subject?: string | undefined;
	keywords?: string | undefined;
	language?: string | undefined;
};
export type RenderOptions = {
	pageFormat?: "A4" | "Letter" | undefined;
	margin?: number | undefined;
	stylesheets?: readonly string[] | undefined;
	fonts?: readonly EmbeddedFont[] | undefined;
	footer?: Element | undefined;
	header?: Element | undefined;
	metadata?: PdfMetadata | undefined;
	tagged?: boolean | undefined;
};

type LoadWasm = () => Promise<{ memory: { buffer: ArrayBufferLike } }>;

export function createRenderer(loadWasm: LoadWasm) {
	return async function render(
		document: Element | Iterable<Element>,
		options?: RenderOptions,
	): Promise<Uint8Array> {
		const tree = await resolveTree(document);
		if (tree.some((node) => node.kind !== "element")) throw new Error("expected element or props");
		const p = props(options ?? {}, [
			"pageFormat",
			"margin",
			"stylesheets",
			"fonts",
			"footer",
			"header",
			"metadata",
			"tagged",
		]);
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
			const writer = new ProtocolWriter(new Binary(renderer, wasm.memory), renderer);
			writer.header(width, height, margin);
			const metadata =
				p.metadata === undefined
					? {}
					: props(p.metadata, ["title", "author", "subject", "keywords", "language"]);
			for (const [key, value] of Object.entries(metadata)) {
				if (value === undefined) continue;
				if (
					typeof value !== "string" ||
					/[\p{Cc}\ud800-\udfff]/u.test(value) ||
					value.length > 4096
				)
					throw new Error(`invalid metadata ${key}`);
			}
			if (
				metadata.language !== undefined &&
				!/^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8})*$/.test(metadata.language as string)
			)
				throw new Error("invalid metadata language");
			if (p.tagged !== undefined && typeof p.tagged !== "boolean")
				throw new Error("invalid tagged option");
			if (p.tagged && !metadata.language) throw new Error("tagged PDF requires metadata.language");
			writer.metadata(metadata, p.tagged === true);
			if (
				p.stylesheets !== undefined &&
				(!Array.isArray(p.stylesheets) || p.stylesheets.some((sheet) => typeof sheet !== "string"))
			)
				throw new Error("invalid stylesheets");
			for (const position of ["header", "footer"] as const) {
				if (p[position] === undefined) continue;
				const repeating = await resolveTree(p[position]);
				if (repeating.length !== 1 || repeating[0]?.kind !== "element")
					throw new Error(`${position} must resolve to one element`);
				writer.repeatStart(position);
				compile(
					resolveStyles(repeating, p.stylesheets as readonly string[] | undefined),
					writer,
					fonts,
					undefined,
					0,
					true,
				);
				writer.repeatEnd();
			}
			compile(resolveStyles(tree, p.stylesheets as readonly string[] | undefined), writer, fonts);
			writer.end();
			consumed = true;
			try {
				return renderer.finish();
			} catch (error) {
				throw asError(error);
			}
		} finally {
			if (!consumed) renderer.free();
		}
	};
}
