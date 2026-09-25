import * as flashpdf from "@pettersen3008/flashpdf";
import type { Element, EmbeddedFont, RenderOptions } from "@pettersen3008/flashpdf";
import * as jsxRuntime from "@pettersen3008/flashpdf/jsx-runtime";
import { Download, ExternalLink, FileText, Link2, Upload } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { transform } from "sucrase";

import { Button, buttonVariants } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

import "./style.css";

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

export default { document: (
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
), options: { metadata: { title: "Acme Consulting invoice", language: "en" }, tagged: true } };
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

const library = { ...flashpdf, ...jsxRuntime };
const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor as new (
	...args: string[]
) => (library: unknown) => Promise<unknown>;
const LIBRARY_IMPORT =
	/import\s*\{([^}]*)\}\s*from\s*["']@pettersen3008\/flashpdf(?:\/jsx(?:-dev)?-runtime)?["'];?/g;

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

function App() {
	const [tsx, setTsx] = useState(DEFAULT_TSX);
	const [css, setCss] = useState(DEFAULT_CSS);
	const [fonts, setFonts] = useState<EmbeddedFont[]>([]);
	const [ready, setReady] = useState(false);
	const [status, setStatus] = useState("Loading playground…");
	const [error, setError] = useState("");
	const [pdfUrl, setPdfUrl] = useState("");
	const [fontNames, setFontNames] = useState("");
	const pdfUrlRef = useRef("");

	useEffect(() => {
		let active = true;
		async function restore() {
			if (location.hash.length > 1) {
				try {
					const shared = JSON.parse(await decodeHash(location.hash.slice(1)));
					if (typeof shared.tsx !== "string" || typeof shared.css !== "string") throw new Error();
					if (active) {
						setTsx(shared.tsx);
						setCss(shared.css);
					}
				} catch {
					if (active) setStatus("Ignored an unreadable share link");
				}
			}
			if (active) setReady(true);
		}
		void restore();
		return () => {
			active = false;
			URL.revokeObjectURL(pdfUrlRef.current);
		};
	}, []);

	useEffect(() => {
		if (!ready) return;
		let active = true;
		setStatus("Rendering PDF…");
		const timer = window.setTimeout(async () => {
			try {
				const run = compile(tsx);
				let document = await run(library);
				if (typeof document === "function") document = await document();
				let options: RenderOptions = {};
				if (document && typeof document === "object" && "document" in document) {
					const result = document as { document: unknown; options?: RenderOptions };
					document = result.document;
					options = result.options ?? {};
				}
				const start = performance.now();
				const pdf = await flashpdf.render(document as Element, {
					pageFormat: "A4",
					margin: 36,
					stylesheets: [css],
					fonts,
					...options,
				});
				if (!active) return;
				const ms = performance.now() - start;
				const pages =
					new TextDecoder("latin1").decode(pdf).match(/\/Type\s*\/Page\b/g)?.length ?? 0;
				URL.revokeObjectURL(pdfUrlRef.current);
				pdfUrlRef.current = URL.createObjectURL(
					new Blob([pdf as BlobPart], { type: "application/pdf" }),
				);
				setPdfUrl(pdfUrlRef.current);
				setStatus(
					`${pages} ${pages === 1 ? "page" : "pages"} · ${pdf.byteLength.toLocaleString()} bytes · ${ms.toFixed(1)} ms`,
				);
				setError("");
			} catch (cause) {
				if (!active) return;
				setError(cause instanceof Error ? cause.message : String(cause));
				setStatus("Render failed");
			}
		}, 300);
		return () => {
			active = false;
			clearTimeout(timer);
		};
	}, [tsx, css, fonts, ready]);

	async function uploadFonts(files: FileList | null) {
		const [regular, bold] = [...(files ?? [])];
		if (!regular) {
			setFonts([]);
			setFontNames("");
			return;
		}
		try {
			setFonts([
				{
					family: "Custom",
					regular: new Uint8Array(await regular.arrayBuffer()),
					bold: bold && new Uint8Array(await bold.arrayBuffer()),
				},
			]);
			setFontNames(bold ? `${regular.name}, ${bold.name}` : regular.name);
		} catch (cause) {
			setError(cause instanceof Error ? cause.message : String(cause));
		}
	}

	async function share() {
		location.hash = await encodeHash(JSON.stringify({ tsx, css }));
		try {
			await navigator.clipboard.writeText(location.href);
			setStatus("Share link copied");
		} catch {
			setStatus("Share link is ready in the address bar");
		}
	}

	return (
		<div className="app-shell">
			<header className="site-header">
				<div className="brand">
					<div className="brand-mark" aria-hidden="true">
						<FileText size={20} strokeWidth={2.2} />
					</div>
					<div>
						<div className="brand-name">
							FlashPDF <span>Playground</span>
						</div>
						<p>Write JSX and CSS. See the PDF as you go.</p>
					</div>
				</div>
				<a
					className="source-link"
					href="https://github.com/pettersen3008/flashpdf"
					target="_blank"
					rel="noreferrer"
				>
					GitHub <ExternalLink size={14} aria-hidden="true" />
				</a>
			</header>

			<div className="action-bar">
				<div className="file-control">
					<label htmlFor="fonts" className={buttonVariants({ variant: "outline" })}>
						<Upload size={16} aria-hidden="true" /> Add fonts
					</label>
					<input
						id="fonts"
						type="file"
						accept=".ttf,font/ttf"
						multiple
						onChange={(event) => void uploadFonts(event.target.files)}
					/>
					<span
						className="font-note"
						title={fontNames || "Upload regular and optional bold .ttf files"}
					>
						{fontNames || "Optional .ttf files · regular, then bold"}
					</span>
				</div>
				<div className="action-buttons">
					<Button variant="outline" onClick={() => void share()}>
						<Link2 size={16} aria-hidden="true" /> Share
					</Button>
					<a
						className={buttonVariants({ variant: "default" })}
						href={pdfUrl || undefined}
						download="flashpdf.pdf"
						aria-disabled={!pdfUrl}
						tabIndex={pdfUrl ? undefined : -1}
					>
						<Download size={16} aria-hidden="true" /> Download PDF
					</a>
				</div>
			</div>

			<main className="workspace">
				<section className="editor-pane" aria-label="PDF source">
					<div className="pane-heading">
						<div>
							<span className="eyebrow">SOURCE</span>
							<h1>Editor</h1>
						</div>
						<span className="pane-hint">Changes render automatically</span>
					</div>
					<Tabs defaultValue="tsx" className="editor-tabs">
						<TabsList variant="line" aria-label="Source files">
							<TabsTrigger value="tsx">Template.tsx</TabsTrigger>
							<TabsTrigger value="css">Stylesheet.css</TabsTrigger>
						</TabsList>
						<TabsContent value="tsx" className="editor-panel">
							<label htmlFor="tsx" className="sr-only">
								Template TSX
							</label>
							<textarea
								id="tsx"
								spellCheck={false}
								value={tsx}
								onChange={(event) => setTsx(event.target.value)}
							/>
						</TabsContent>
						<TabsContent value="css" className="editor-panel">
							<label htmlFor="css" className="sr-only">
								Stylesheet CSS
							</label>
							<textarea
								id="css"
								spellCheck={false}
								value={css}
								onChange={(event) => setCss(event.target.value)}
							/>
						</TabsContent>
					</Tabs>
					<p className="editor-footer">
						Export an element, function, or <code>{"{ document, options }"}</code>. Use{" "}
						<code>fontFamily: "Custom"</code> for uploaded fonts.
					</p>
				</section>

				<section className="preview-pane" aria-label="PDF output">
					<div className="pane-heading">
						<div>
							<span className="eyebrow">OUTPUT</span>
							<h2>Preview</h2>
						</div>
						<span className={error ? "render-state is-error" : "render-state"}>
							{error ? "Error" : "Live"}
						</span>
					</div>
					<p className="render-meta" role="status" aria-live="polite">
						{status}
					</p>
					{error && (
						<pre className="error-box" role="alert">
							{error}
						</pre>
					)}
					<div className="preview-canvas">
						{pdfUrl ? (
							<iframe src={pdfUrl} title="PDF preview" />
						) : (
							<div className="preview-empty">Your PDF will appear here.</div>
						)}
					</div>
				</section>
			</main>
		</div>
	);
}

createRoot(document.getElementById("root")!).render(<App />);
