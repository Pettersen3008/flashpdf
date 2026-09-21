import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { createRequire } from "node:module";
import { cpus, platform, release, totalmem } from "node:os";
import { dirname, join } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";

const libraries = ["flashpdf", "pdfkit", "react-pdf", "takumi"];
const fixtures = ["invoice", "report"];
const packageNames = {
	flashpdf: "@pettersen3008/flashpdf",
	pdfkit: "pdfkit",
	"react-pdf": "@react-pdf/renderer",
	takumi: "takumi-pdf",
};

if (process.argv[2] === "--child") {
	await runChild(process.argv[3], process.argv[4], Number(process.argv[5]));
} else {
	runParent();
}

async function runChild(library, fixture, warmRuns) {
	if (
		!libraries.includes(library) ||
		!fixtures.includes(fixture) ||
		!Number.isInteger(warmRuns) ||
		warmRuns < 1
	) {
		throw new Error("invalid benchmark child arguments");
	}

	const rssBefore = process.memoryUsage().rss;
	const coldStart = performance.now();
	const render = await createRenderer(library, fixture);
	const coldPdf = await render();
	const coldMilliseconds = performance.now() - coldStart;
	const warmMilliseconds = [];
	let pdf = coldPdf;
	for (let run = 0; run < warmRuns; run++) {
		global.gc?.();
		const start = performance.now();
		pdf = await render();
		warmMilliseconds.push(performance.now() - start);
	}

	console.log(
		JSON.stringify({
			coldMilliseconds,
			warmMilliseconds,
			peakRssBytes: process.resourceUsage().maxRSS * 1024,
			rssBeforeBytes: rssBefore,
			pdfBytes: pdf.length,
		}),
	);
}

function runParent() {
	const coldRuns = positiveInteger("FLASHPDF_BENCH_COLD_RUNS", 3);
	const warmRuns = positiveInteger("FLASHPDF_BENCH_RUNS", 10);
	const results = [];
	for (const fixture of fixtures) {
		for (const library of libraries) {
			const samples = Array.from({ length: coldRuns }, () =>
				runIsolated(library, fixture, warmRuns),
			);
			results.push({
				fixture,
				library,
				coldMilliseconds: roundedMedian(samples.map((sample) => sample.coldMilliseconds)),
				warmMilliseconds: roundedMedian(samples.flatMap((sample) => sample.warmMilliseconds)),
				peakRssBytes: median(samples.map((sample) => sample.peakRssBytes)),
				peakRssDeltaBytes: median(
					samples.map((sample) => Math.max(0, sample.peakRssBytes - sample.rssBeforeBytes)),
				),
				pdfBytes: median(samples.map((sample) => sample.pdfBytes)),
			});
		}
	}

	console.log(
		JSON.stringify(
			{
				measuredAt: new Date().toISOString(),
				environment: {
					node: process.version,
					v8: process.versions.v8,
					platform: `${platform()} ${release()}`,
					arch: process.arch,
					cpu: cpus()[0]?.model ?? "unknown",
					logicalCpus: cpus().length,
					totalMemoryBytes: totalmem(),
				},
				runs: { cold: coldRuns, warmPerProcess: warmRuns },
				measurements: {
					coldMilliseconds:
						"module import, initialization, authoring, and first render in a fresh process",
					warmMilliseconds: "authoring and render after one completed render",
					peakRssBytes: "maximum resident set size of the isolated process",
					peakRssDeltaBytes: "peak RSS minus RSS before importing the renderer",
					packageBytes: "installed top-level package files only, including WASM",
					wasmBytes: "WASM files within that top-level package",
				},
				packages: Object.fromEntries(libraries.map((library) => [library, packageStats(library)])),
				results,
			},
			null,
			2,
		),
	);
}

function runIsolated(library, fixture, warmRuns) {
	const child = spawnSync(
		process.execPath,
		["--expose-gc", fileURLToPath(import.meta.url), "--child", library, fixture, String(warmRuns)],
		{ encoding: "utf8", maxBuffer: 1024 * 1024 * 10 },
	);
	if (child.status !== 0)
		throw new Error(`${library}/${fixture} failed:\n${child.stderr || child.stdout}`);
	return JSON.parse(child.stdout.trim().split("\n").at(-1));
}

async function createRenderer(library, fixture) {
	if (library === "flashpdf") return createFlashPdfRenderer(fixture);
	if (library === "pdfkit") return createPdfKitRenderer(fixture);
	if (library === "react-pdf") return createReactPdfRenderer(fixture);
	return createTakumiRenderer(fixture);
}

async function createFlashPdfRenderer(fixture) {
	const [{ render }, { jsx, jsxs }] = await Promise.all([
		import("../dist/index.js"),
		import("../dist/jsx-runtime.js"),
	]);
	const row = { display: "flex", flexDirection: "row", fontSize: "9pt" };
	return () =>
		render(
			jsxs("main", {
				children:
					fixture === "invoice"
						? [
								jsx("h1", { children: "Invoice" }),
								...Array.from({ length: 80 }, (_, index) =>
									jsxs("div", {
										style: row,
										children: [
											jsx("span", { style: { flex: 1 }, children: `Consulting item ${index + 1}` }),
											jsx("span", {
												style: { width: "70pt", textAlign: "right" },
												children: "$100.00",
											}),
										],
									}),
								),
							]
						: [
								jsx("h1", { children: "Quarterly report" }),
								...Array.from({ length: 220 }, (_, index) =>
									jsx("p", {
										children: `Section ${index + 1}. Revenue remained stable across the reporting period.`,
									}),
								),
							],
			}),
		);
}

async function createPdfKitRenderer(fixture) {
	const { default: PDFDocument } = await import("pdfkit");
	return () =>
		new Promise((resolve, reject) => {
			const document = new PDFDocument({ size: "A4", margin: 36 });
			const chunks = [];
			document.on("data", (chunk) => chunks.push(chunk));
			document.on("end", () => resolve(Buffer.concat(chunks)));
			document.on("error", reject);
			document
				.fontSize(24)
				.text(fixture === "invoice" ? "Invoice" : "Quarterly report")
				.moveDown();
			const count = fixture === "invoice" ? 80 : 220;
			for (let index = 0; index < count; index++) {
				if (fixture === "invoice") {
					document
						.fontSize(9)
						.text(`Consulting item ${index + 1}`, { continued: true })
						.text("$100.00", { align: "right" });
				} else {
					document
						.fontSize(12)
						.text(`Section ${index + 1}. Revenue remained stable across the reporting period.`)
						.moveDown(0.5);
				}
			}
			document.end();
		});
}

async function createReactPdfRenderer(fixture) {
	const [{ default: React }, { Document, Page, StyleSheet, Text, View, renderToBuffer }] =
		await Promise.all([import("react"), import("@react-pdf/renderer")]);
	const h = React.createElement;
	const styles = StyleSheet.create({
		page: { padding: 36, fontSize: 12 },
		heading: { fontSize: 24, marginBottom: 16 },
		row: { flexDirection: "row", gap: 12, fontSize: 9 },
		name: { flexGrow: 1 },
		amount: { width: 70, textAlign: "right" },
		paragraph: { marginBottom: 8 },
	});
	return () =>
		renderToBuffer(
			h(
				Document,
				null,
				h(
					Page,
					{ size: "A4", style: styles.page },
					h(
						Text,
						{ style: styles.heading },
						fixture === "invoice" ? "Invoice" : "Quarterly report",
					),
					...(fixture === "invoice"
						? Array.from({ length: 80 }, (_, index) =>
								h(
									View,
									{ key: index, style: styles.row },
									h(Text, { style: styles.name }, `Consulting item ${index + 1}`),
									h(Text, { style: styles.amount }, "$100.00"),
								),
							)
						: Array.from({ length: 220 }, (_, index) =>
								h(
									Text,
									{ key: index, style: styles.paragraph },
									`Section ${index + 1}. Revenue remained stable across the reporting period.`,
								),
							)),
				),
			),
		);
}

async function createTakumiRenderer(fixture) {
	const [{ default: React }, { render }] = await Promise.all([
		import("react"),
		import("takumi-pdf"),
	]);
	const h = React.createElement;
	return () =>
		render(
			h(
				"main",
				null,
				h("h1", null, fixture === "invoice" ? "Invoice" : "Quarterly report"),
				...(fixture === "invoice"
					? Array.from({ length: 80 }, (_, index) =>
							h(
								"div",
								{ key: index, style: { display: "flex", fontSize: 12 } },
								h("span", { style: { flex: 1 } }, `Consulting item ${index + 1}`),
								h("span", null, "$100.00"),
							),
						)
					: Array.from({ length: 220 }, (_, index) =>
							h(
								"p",
								{ key: index },
								`Section ${index + 1}. Revenue remained stable across the reporting period.`,
							),
						)),
			),
			{ size: "a4", margin: 48 },
		);
}

function positiveInteger(name, fallback) {
	const value = Number(process.env[name] ?? fallback);
	if (!Number.isInteger(value) || value < 1) throw new Error(`${name} must be a positive integer`);
	return value;
}

function median(values) {
	const sorted = values.toSorted((left, right) => left - right);
	const middle = Math.floor(sorted.length / 2);
	return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

function roundedMedian(values) {
	return Number(median(values).toFixed(2));
}

function packageStats(library) {
	const root = packageRoot(library);
	const files =
		library === "flashpdf"
			? ["package.json", "README.md", "CHANGELOG.md", "LICENSE", "dist", "wasm"]
			: ["."];
	return {
		version: JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version,
		packageBytes: files.reduce((sum, file) => sum + pathBytes(join(root, file)), 0),
		wasmBytes: wasmBytes(root),
	};
}

function packageRoot(library) {
	if (library === "flashpdf") return dirname(dirname(fileURLToPath(import.meta.url)));
	const require = createRequire(import.meta.url);
	let current = dirname(require.resolve(packageNames[library]));
	while (current !== dirname(current)) {
		const manifest = join(current, "package.json");
		if (
			existsSync(manifest) &&
			JSON.parse(readFileSync(manifest, "utf8")).name === packageNames[library]
		)
			return current;
		current = dirname(current);
	}
	throw new Error(`package root not found for ${library}`);
}

function pathBytes(path) {
	if (!existsSync(path)) return 0;
	const stat = statSync(path);
	if (stat.isFile()) return stat.size;
	return readdirSync(path, { withFileTypes: true }).reduce(
		(sum, entry) => sum + (entry.isSymbolicLink() ? 0 : pathBytes(join(path, entry.name))),
		0,
	);
}

function wasmBytes(root) {
	if (!existsSync(root)) return 0;
	return readdirSync(root, { withFileTypes: true }).reduce((sum, entry) => {
		if (entry.isSymbolicLink()) return sum;
		const path = join(root, entry.name);
		if (entry.isDirectory()) return sum + wasmBytes(path);
		return sum + (entry.name.endsWith(".wasm") ? statSync(path).size : 0);
	}, 0);
}
