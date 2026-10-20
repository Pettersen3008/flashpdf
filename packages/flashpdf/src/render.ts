import { PdfRenderer } from "../wasm/flashpdf_wasm.js";
import { props } from "./assert.js";
import { asError, Binary } from "./binary.js";
import { compile, compileBody } from "./compile.js";
import { styleResolver } from "./css.js";
import type { Edges, Element, Length } from "./element.js";
import { registerFonts } from "./fonts.js";
import { pageSetup } from "./page.js";
import { ProtocolWriter } from "./protocol.js";
import { pageOverflow, resolveTree, type Node } from "./tree.js";

/** A family without an italic face for a weight slants its upright face instead. */
export type EmbeddedFont = {
	family: string;
	regular: Uint8Array;
	bold?: Uint8Array | undefined;
	italic?: Uint8Array | undefined;
	boldItalic?: Uint8Array | undefined;
};
export type PdfMetadata = {
	title?: string | undefined;
	author?: string | undefined;
	subject?: string | undefined;
	keywords?: string | undefined;
	language?: string | undefined;
};
export type RenderOptions = {
	/** A named size, or a custom size where a number is points. */
	pageFormat?:
		| "A3"
		| "A4"
		| "A5"
		| "Letter"
		| "Legal"
		| { width: Length; height: Length }
		| undefined;
	/** Puts the longer side of the page size across (landscape) or down (portrait). */
	orientation?: "portrait" | "landscape" | undefined;
	/** One to four lengths in CSS shorthand order, or each side. */
	margin?: Edges | { top: Length; right: Length; bottom: Length; left: Length } | undefined;
	stylesheets?: readonly string[] | undefined;
	fonts?: readonly EmbeddedFont[] | undefined;
	/** Paints on every page above the body; its height counts each page number as five digits. */
	header?: Element | Iterable<Element> | undefined;
	/** Paints on every page below the body; its height counts each page number as five digits. */
	footer?: Element | Iterable<Element> | undefined;
	/** Replaces `header` on the first page; `null` leaves the first page without one. */
	firstPageHeader?: Element | Iterable<Element> | null | undefined;
	/** Replaces `footer` on the first page; `null` leaves the first page without one. */
	firstPageFooter?: Element | Iterable<Element> | null | undefined;
	metadata?: PdfMetadata | undefined;
	tagged?: boolean | undefined;
	/** Bookmarks every `h1` to `h6` in document order, nested by level, and opens the outline panel. */
	outline?: boolean | undefined;
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
/** Option, the block it fills, and whether that block replaces the other on the first page. */
const FURNITURE = [
	["header", "header", false],
	["footer", "footer", false],
	["firstPageHeader", "header", true],
	["firstPageFooter", "footer", true],
] as const;

type LoadWasm = () => Promise<{ memory: { buffer: ArrayBufferLike } }>;

export function createRenderer(loadWasm: LoadWasm) {
	return async function render(
		document: Element | Iterable<Element>,
		options?: RenderOptions,
	): Promise<Uint8Array> {
		const p = props(options ?? {}, [
			"pageFormat",
			"orientation",
			"margin",
			"stylesheets",
			"fonts",
			...FURNITURE.map(([option]) => option),
			"metadata",
			"tagged",
			"outline",
			"pdfa",
			"attachments",
			"facturx",
		]);
		if (
			p.stylesheets !== undefined &&
			(!Array.isArray(p.stylesheets) || p.stylesheets.some((sheet) => typeof sheet !== "string"))
		)
			throw new Error("invalid stylesheets");
		const page = pageSetup(
			p.pageFormat,
			p.orientation,
			p.margin,
			p.stylesheets as readonly string[] | undefined,
		);
		const metadata =
			p.metadata === undefined
				? {}
				: props(p.metadata, ["title", "author", "subject", "keywords", "language"]);
		for (const [key, value] of Object.entries(metadata)) {
			if (value === undefined) continue;
			if (typeof value !== "string" || /[\p{Cc}\ud800-\udfff]/u.test(value) || value.length > 4096)
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
		if (p.outline !== undefined && typeof p.outline !== "boolean")
			throw new Error("invalid outline option");
		// Options are checked before user components run; fonts need the engine to parse.
		const tree = await resolveTree(document);
		if (tree.some((node) => node.kind !== "element")) throw new Error("expected element or props");
		const repeating: ["header" | "footer", boolean, string, readonly Node[]][] = [];
		for (const [option, position, first] of FURNITURE) {
			if (p[option] === undefined) continue;
			const nodes = await resolveTree(p[option]);
			if (nodes.some((node) => node.kind !== "element"))
				throw new Error(`${option} expects elements; wrap its text in one`);
			repeating.push([position, first, option, nodes]);
		}
		const wasm = await loadWasm();
		const styles = styleResolver(
			p.stylesheets as readonly string[] | undefined,
			page.width - page.margin[1]! - page.margin[3]!,
		);
		// The second pass pushes the record the engine rejected alone, so the element that wrote it reports.
		const emit = (replayAt?: number): Uint8Array => {
			const renderer = new PdfRenderer();
			const binary = new Binary(renderer, wasm.memory, replayAt);
			let consumed = false;
			try {
				const fonts = registerFonts(renderer, p.fonts);
				const writer = new ProtocolWriter(binary, renderer);
				writer.header(page.width, page.height, page.margin);
				writer.metadata(metadata, p.tagged === true);
				attach(renderer, writer, p.pdfa, p.attachments, p.facturx);
				for (const [position, first, option, nodes] of repeating) {
					try {
						writer.repeatStart(position, first, option);
						compile(styles(nodes, true), writer, fonts, undefined, 0, true);
						writer.repeatEnd();
						binary.flush();
					} catch (error) {
						if (error instanceof Error && error.message === "PageOverflow")
							throw new Error(
								`PageOverflow: with ${option}, the header and footer take more than half the page`,
								{ cause: error },
							);
						throw error;
					}
				}
				compileBody(styles(tree), writer, fonts, p.outline === true);
				writer.end();
				consumed = true;
				try {
					return renderer.finish();
				} catch (error) {
					throw asError(error);
				}
			} catch (error) {
				const failed = consumed ? undefined : binary.failure();
				if (failed !== undefined && replayAt === undefined) return emit(failed);
				throw pageOverflow(error);
			} finally {
				if (!consumed) renderer.free();
			}
		};
		return emit();
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
