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
	pdfa?: "3b" | undefined;
	attachments?: readonly Attachment[] | undefined;
	facturx?: FacturX | undefined;
};
export type Attachment = {
	name: string;
	data: Uint8Array;
	mimeType: string;
	description?: string | undefined;
	relationship: (typeof RELATIONSHIPS)[number];
};
export type FacturX = { xml: Uint8Array; profile: (typeof PROFILES)[number] };

const RELATIONSHIPS = ["Source", "Data", "Alternative", "Supplement", "Unspecified"] as const;
const PROFILES = ["MINIMUM", "BASIC WL", "BASIC", "EN 16931", "EXTENDED", "XRECHNUNG"] as const;

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
			"pdfa",
			"attachments",
			"facturx",
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
			attach(renderer, writer, p.pdfa, p.attachments, p.facturx);
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

/** Writes the PDF/A and attachment records before the body; the core repeats each check. */
function attach(
	renderer: PdfRenderer,
	writer: ProtocolWriter,
	pdfa: unknown,
	attachments: unknown,
	facturx: unknown,
) {
	if (pdfa !== undefined && pdfa !== "3b")
		throw new Error('invalid pdfa option; the supported level is "3b"');
	let bytes = 0;
	const add = (data: Uint8Array) => {
		if ((bytes += data.byteLength) > 64 * 1024 * 1024)
			throw new Error("attachments exceed 64 MiB per render");
		try {
			return renderer.add_attachment(data);
		} catch (error) {
			throw asError(error);
		}
	};
	if (facturx !== undefined) {
		const options = props(facturx, ["xml", "profile"]);
		const profile = PROFILES.indexOf(options.profile as FacturX["profile"]);
		if (profile < 0) throw new Error(`invalid facturx profile; use one of ${PROFILES.join(", ")}`);
		if (!(options.xml instanceof Uint8Array) || !looksLikeXml(options.xml))
			throw new Error("facturx xml must be UTF-8 XML bytes");
		writer.pdfa(profile + 1, add(options.xml));
	} else if (pdfa) writer.pdfa(0, 0);
	if (attachments === undefined) return;
	if (!Array.isArray(attachments)) throw new Error("invalid attachments");
	const names = new Set<string>();
	for (const entry of attachments) {
		const file = props(entry, ["name", "data", "mimeType", "description", "relationship"]);
		const { name, mimeType, description = "" } = file;
		if (typeof name !== "string" || !/^[^\p{Cc}\ud800-\udfff/\\]{1,255}$/u.test(name))
			throw new Error("attachment name must be 1 to 255 characters without controls or slashes");
		if (names.has(name)) throw new Error(`duplicate attachment name: ${name}`);
		names.add(name);
		if (!(file.data instanceof Uint8Array))
			throw new Error(`attachment ${name}: data must be a Uint8Array`);
		if (
			typeof mimeType !== "string" ||
			mimeType.length > 255 ||
			!/^[\w.+-]+\/[\w.+-]+$/.test(mimeType)
		)
			throw new Error(`attachment ${name}: invalid mimeType`);
		if (
			typeof description !== "string" ||
			/[\p{Cc}\ud800-\udfff]/u.test(description) ||
			description.length > 4096
		)
			throw new Error(`attachment ${name}: invalid description`);
		const relationship = RELATIONSHIPS.indexOf(file.relationship as Attachment["relationship"]);
		if (relationship < 0)
			throw new Error(
				`attachment ${name}: relationship must be one of ${RELATIONSHIPS.join(", ")}`,
			);
		writer.attachment(add(file.data), relationship, name, mimeType, description);
	}
}

/** A shape check cheap enough for the boundary, not a parse. */
function looksLikeXml(bytes: Uint8Array): boolean {
	try {
		const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes).trim();
		return text.startsWith("<") && text.endsWith(">");
	} catch {
		return false;
	}
}
