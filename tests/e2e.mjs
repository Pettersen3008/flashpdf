import { execFileSync, spawnSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const work = mkdtempSync(join(tmpdir(), "flashpdf-e2e-"));
const nodeOnly = process.argv.includes("--node-only");
const run = (command, args, options = {}) =>
	execFileSync(command, args, { cwd: work, encoding: "utf8", stdio: "pipe", ...options });

try {
	const tarball = run("npm", ["pack", "--pack-destination", work, "--silent"], {
		cwd: join(repo, "packages/flashpdf"),
	})
		.trim()
		.split("\n")
		.at(-1);
	writeFileSync(
		join(work, "package.json"),
		JSON.stringify({ name: "flashpdf-consumer", private: true, type: "module" }),
	);
	run("npm", ["install", "--silent", "--no-audit", "--no-fund", join(work, tarball)]);
	cpSync(join(repo, "packages/flashpdf/test/fixtures/Abel-Regular.ttf"), join(work, "font.ttf"));

	writeFileSync(
		join(work, "consumer.mjs"),
		`import { readFile } from "node:fs/promises";
import { jsx, jsxs } from "@pettersen3008/flashpdf/jsx-runtime";
import { render, stylesheet } from "@pettersen3008/flashpdf";

const Invoice = ({ total }) => jsxs("main", {
  style: { fontFamily: "Invoice" },
  children: [jsx("h1", { children: "Invoice" }), jsx("p", { className: "total", children: total })],
});
const font = new Uint8Array(await readFile("font.ttf"));
const pdf = await render(jsx(Invoice, { total: "$100.00" }), {
  pageFormat: "A4",
  margin: 36,
  stylesheets: [stylesheet(".total { color: #0000ff; text-align: right }")],
  fonts: [{ family: "Invoice", regular: font, bold: font }],
});
if (!(pdf instanceof Uint8Array)) throw new Error("render must return a Uint8Array");
if ((Buffer.from(pdf).toString("latin1").match(/\\/Subtype \\/TrueType/g) ?? []).length !== 2)
  throw new Error("embedded regular and bold fonts missing");
process.stdout.write(Buffer.from(pdf).toString("base64"));
`,
	);

	const nodePdf = run(process.execPath, ["consumer.mjs"]);
	if (!nodePdf.startsWith("JVBER")) throw new Error("Node did not render a PDF");
	console.log("node: ok");

	writeFileSync(
		join(work, "lambda.mjs"),
		`import { jsx } from "@pettersen3008/flashpdf/jsx-runtime";
import { render } from "@pettersen3008/flashpdf";
export async function handler() {
  const pdf = await render(jsx("main", { children: "Lambda invoice" }));
  return { statusCode: 200, isBase64Encoded: true, body: Buffer.from(pdf).toString("base64") };
}
`,
	);
	const lambdaPdf = run(process.execPath, [
		"--input-type=module",
		"-e",
		`const { handler } = await import("./lambda.mjs"); const response = await handler(); process.stdout.write(response.body);`,
	]);
	if (!lambdaPdf.startsWith("JVBER")) throw new Error("Lambda handler did not render a PDF");
	console.log("aws-lambda: ok");

	if (!nodeOnly && spawnSync("bun", ["--version"]).status === 0) {
		const bunPdf = run("bun", ["consumer.mjs"]);
		if (bunPdf !== nodePdf) throw new Error("Bun output differs from Node");
		console.log("bun: ok");
	} else if (!nodeOnly) console.warn("bun not installed; Bun support is unverified on this target");

	if (!nodeOnly) {
		writeFileSync(
			join(work, "worker.mjs"),
			`import { jsx } from "@pettersen3008/flashpdf/jsx-runtime";
import { render } from "@pettersen3008/flashpdf";
export default {
  async fetch() {
    return new Response(await render(jsx("main", { children: "Worker invoice" })), {
      headers: { "content-type": "application/pdf" },
    });
  },
};
`,
		);
		writeFileSync(
			join(work, "wrangler.jsonc"),
			JSON.stringify({
				name: "flashpdf-smoke",
				main: "worker.mjs",
				compatibility_date: "2026-09-18",
			}),
		);
		const { unstable_startWorker } = await import("wrangler");
		const worker = await unstable_startWorker({ config: join(work, "wrangler.jsonc") });
		try {
			const response = await worker.fetch("http://example.com");
			const workerPdf = Buffer.from(await response.arrayBuffer()).toString("latin1");
			if (!workerPdf.startsWith("%PDF-")) throw new Error("Cloudflare Worker did not render a PDF");
			console.log("cloudflare-worker: ok");
		} finally {
			await worker.dispose();
		}

		const resolved = [
			"@pettersen3008/flashpdf",
			"@pettersen3008/flashpdf/package.json",
			"@pettersen3008/flashpdf/jsx-runtime",
		];
		for (const entry of resolved)
			run(process.execPath, [
				"--input-type=module",
				"-e",
				`console.log(import.meta.resolve(${JSON.stringify(entry)}))`,
			]);

		const types = join(work, "types");
		cpSync(join(repo, "packages/flashpdf/examples"), types, { recursive: true });
		writeFileSync(
			join(types, "consumer.tsx"),
			`import { render, stylesheet, type EmbeddedFont, type RenderOptions } from "@pettersen3008/flashpdf";

const options: RenderOptions = { pageFormat: "A4", margin: 36 };
const font: EmbeddedFont = { family: "Invoice", regular: new Uint8Array(), bold: new Uint8Array() };
export function invoice(total: string) {
  return render(<main><h1>Invoice</h1><p className="total">{total}</p></main>, {
    ...options,
    stylesheets: [stylesheet(".total { text-align: right }")],
    fonts: [font],
  });
}
`,
		);
		writeFileSync(
			join(types, "tsconfig.json"),
			JSON.stringify({
				compilerOptions: {
					strict: true,
					outDir: "built",
					module: "esnext",
					target: "es2022",
					moduleResolution: "bundler",
					jsx: "react-jsx",
					jsxImportSource: "@pettersen3008/flashpdf",
				},
				include: ["consumer.tsx", "native-invoice.tsx"],
			}),
		);
		run("pnpm", ["exec", "tsc", "-p", join(types, "tsconfig.json")], { cwd: repo });
		run(process.execPath, [
			"--input-type=module",
			"-e",
			`import { nativeInvoice } from "./types/built/native-invoice.js";
const pdf = await nativeInvoice({
  number: "2026-0042", issued: "2026-09-01", due: "2026-09-30", currency: "EUR", taxRate: 0.25,
  seller: { name: "Acme Supply Co.", address: "1 Harbour Road, Oslo" },
  customer: { name: "Beta Industries", address: "9 Market Street, Bergen" },
  items: Array.from({ length: 60 }, (_, index) => ({ description: "Line item " + (index + 1), quantity: index + 1, unitPrice: 12.5 })),
});
if (Buffer.from(pdf.subarray(0, 5)).toString() !== "%PDF-") throw new Error("invoice example did not render");`,
		]);
		console.log("exports, JSX types, and invoice example: ok");

		writeFileSync(
			join(work, "index.html"),
			'<!doctype html><html><body><pre id="out">rendering</pre><script type="module" src="/src.js"></script></body></html>',
		);
		writeFileSync(
			join(work, "src.js"),
			`import { jsx, jsxs } from "@pettersen3008/flashpdf/jsx-runtime";
import { render } from "@pettersen3008/flashpdf";
const Invoice = () => jsxs("main", { children: [jsx("h1", { children: "Invoice" }), jsx("p", { children: "$100.00" })] });
const pdf = await render(jsx(Invoice, {}));
document.querySelector("#out").textContent = new TextDecoder().decode(pdf.subarray(0, 5)) + " " + pdf.length;
`,
		);
		const { chromium } = await import("playwright");
		const { build, preview } = await import("vite");
		await build({ root: work, logLevel: "warn" });
		const assets = join(work, "dist/assets");
		const names = readdirSync(assets);
		if (!names.some((name) => name.endsWith(".wasm")))
			throw new Error("Vite emitted no WASM asset");
		const bundle = names
			.filter((name) => name.endsWith(".js"))
			.map((name) => readFileSync(join(assets, name), "utf8"))
			.join("\n");
		if (bundle.includes("lightningcss")) throw new Error("browser bundle reaches Lightning CSS");
		if (bundle.includes("externalized for browser"))
			throw new Error("browser bundle uses Node builtins");

		const server = await preview({
			root: work,
			logLevel: "error",
			preview: { host: "127.0.0.1", port: 0 },
		});
		const address = server.httpServer.address();
		const browser = await chromium.launch({ headless: true });
		try {
			const page = await browser.newPage();
			await page.goto(`http://127.0.0.1:${address.port}`, { waitUntil: "networkidle" });
			await page.waitForFunction(() =>
				document.querySelector("#out")?.textContent?.startsWith("%PDF-"),
			);
			console.log(`browser: ${await page.locator("#out").textContent()}`);
		} finally {
			await browser.close();
			server.httpServer.close();
		}
	}
} finally {
	rmSync(work, { recursive: true, force: true });
}
