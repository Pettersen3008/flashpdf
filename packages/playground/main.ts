import * as flashpdf from "@pettersen3008/flashpdf";
import type { Element, EmbeddedFont } from "@pettersen3008/flashpdf";
import * as jsxRuntime from "@pettersen3008/flashpdf/jsx-runtime";
import { transform } from "sucrase";

const DEFAULT_TSX = `const invoice = {
	title: "Acme Consulting",
	number: "2026-0042",
	dueDate: "15 Oct 2026",
	items: [
		{ id: "1", name: "Discovery workshop", total: "EUR 1 200.00" },
		{ id: "2", name: "Implementation, 3 days", total: "EUR 3 600.00" },
		{ id: "3", name: "Handover and documentation", total: "EUR 800.00" },
	],
};

function Item({ name, total }: { name: string; total: string }) {
	return (
		<div className="item">
			<span>{name}</span>
			<span className="amount">{total}</span>
		</div>
	);
}

export default (
	<main className="invoice">
		<header className="header">
			<div>
				<h1>{invoice.title}</h1>
				<p className="muted">
					Invoice <b>#{invoice.number}</b>
				</p>
			</div>
			<p style={{ textAlign: "right", fontSize: 12 }}>
				Due <b>{invoice.dueDate}</b>
			</p>
		</header>
		<section className="items">
			{invoice.items.map((item) => (
				<Item key={item.id} name={item.name} total={item.total} />
			))}
		</section>
		<hr />
		<div className="item total">
			<span>Total due</span>
			<span className="amount">EUR 5 600.00</span>
		</div>
	</main>
);
`;

const DEFAULT_CSS = `.invoice {
	--muted: #667085;
	color: #222;
	font-size: 11pt;
}
.header {
	display: flex;
	flex-direction: row;
}
.header > div {
	flex: 1;
}
.muted {
	color: var(--muted);
	font-size: 10pt;
}
.item {
	display: flex;
	flex-direction: row;
	gap: 0;
}
.item > span {
	flex: 1;
}
.amount {
	width: 80pt;
	text-align: right;
}
.total {
	font-weight: bold;
}
`;

const byId = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const tsx = byId<HTMLTextAreaElement>("tsx");
const css = byId<HTMLTextAreaElement>("css");
const preview = byId<HTMLIFrameElement>("preview");
const status = byId("status");
const errorBox = byId("error");
const download = byId<HTMLAnchorElement>("download");
const share = byId<HTMLButtonElement>("share");
const fontInput = byId<HTMLInputElement>("fonts");

const library = { ...flashpdf, ...jsxRuntime };
const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor as new (
	...args: string[]
) => (library: unknown) => Promise<unknown>;
const LIBRARY_IMPORT =
	/import\s*\{([^}]*)\}\s*from\s*["']@pettersen3008\/flashpdf(?:\/jsx(?:-dev)?-runtime)?["'];?/g;

// sucrase emits ESM; imports from the library become destructuring of the
// bundled module and `export default` becomes the function's return value.
function compile(source: string) {
	const body = transform(source, {
		transforms: ["typescript", "jsx"],
		jsxRuntime: "automatic",
		jsxImportSource: "@pettersen3008/flashpdf",
		production: true,
		filePath: "template.tsx",
	})
		.code.replace(
			LIBRARY_IMPORT,
			(_, names: string) => `const {${names.replaceAll(/\s+as\s+/g, ": ")}} = __flashpdf;`,
		)
		.replace(/^export default /m, "const __default = ")
		.replace(/^export (?=(?:const|let|var|function|class|async) )/gm, "");
	if (/^import\b/m.test(body)) {
		throw new Error('Only imports from "@pettersen3008/flashpdf" are available in the playground.');
	}
	return new AsyncFunction("__flashpdf", `"use strict";\n${body}\nreturn __default;`);
}

let fonts: EmbeddedFont[] = [];
let pdfUrl = "";
let generation = 0;
let timer = 0;

async function update() {
	const id = ++generation;
	try {
		const run = compile(tsx.value);
		let document = await run(library);
		if (typeof document === "function") document = await document();
		const start = performance.now();
		const pdf = await flashpdf.render(document as Element, {
			pageFormat: "A4",
			margin: 36,
			stylesheets: [css.value],
			fonts,
		});
		const ms = performance.now() - start;
		if (id !== generation) return;
		const pages = new TextDecoder("latin1").decode(pdf).match(/\/Type\s*\/Page\b/g)?.length ?? 0;
		URL.revokeObjectURL(pdfUrl);
		pdfUrl = URL.createObjectURL(new Blob([pdf as BlobPart], { type: "application/pdf" }));
		preview.src = pdfUrl;
		download.href = pdfUrl;
		status.textContent = `Rendered in ${ms.toFixed(1)} ms, ${pdf.byteLength.toLocaleString()} bytes, ${pages} ${pages === 1 ? "page" : "pages"}`;
		errorBox.hidden = true;
		errorBox.textContent = "";
	} catch (error) {
		if (id !== generation) return;
		errorBox.textContent = error instanceof Error ? error.message : String(error);
		errorBox.hidden = false;
		status.textContent = "Render failed";
	}
}

function schedule() {
	clearTimeout(timer);
	timer = window.setTimeout(update, 300);
}

const base64url = (bytes: Uint8Array) =>
	bytes.toBase64({ alphabet: "base64url", omitPadding: true });

async function encodeHash(text: string) {
	const bytes = new TextEncoder().encode(text);
	if (!("CompressionStream" in window)) return `p${base64url(bytes)}`;
	const stream = new Blob([bytes]).stream().pipeThrough(new CompressionStream("deflate-raw"));
	return `z${base64url(new Uint8Array(await new Response(stream).arrayBuffer()))}`;
}

async function decodeHash(hash: string) {
	const bytes = Uint8Array.fromBase64(hash.slice(1), { alphabet: "base64url" });
	if (hash.startsWith("p")) return new TextDecoder().decode(bytes);
	const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
	return new Response(stream).text();
}

tsx.addEventListener("input", schedule);
css.addEventListener("input", schedule);

fontInput.addEventListener("change", async () => {
	const [regular, bold] = [...(fontInput.files ?? [])];
	fonts = regular
		? [
				{
					family: "Custom",
					regular: new Uint8Array(await regular.arrayBuffer()),
					bold: bold && new Uint8Array(await bold.arrayBuffer()),
				},
			]
		: [];
	void update();
});

share.addEventListener("click", async () => {
	location.hash = await encodeHash(JSON.stringify({ tsx: tsx.value, css: css.value }));
	await navigator.clipboard.writeText(location.href).catch(() => {});
	status.textContent = "Share link copied to the clipboard and set in the address bar";
});

tsx.value = DEFAULT_TSX;
css.value = DEFAULT_CSS;
if (location.hash.length > 1) {
	try {
		const shared = JSON.parse(await decodeHash(location.hash.slice(1)));
		tsx.value = shared.tsx;
		css.value = shared.css;
	} catch {
		status.textContent = "Ignored an unreadable share link";
	}
}
void update();
