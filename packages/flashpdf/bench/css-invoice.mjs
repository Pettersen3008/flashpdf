import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, realpathSync, statSync } from "node:fs";
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
			const coldSamples = samples.map((sample) => sample.coldMilliseconds);
			const warmSamples = samples.flatMap((sample) => sample.warmMilliseconds);
			results.push({
				fixture,
				library,
				coldMilliseconds: roundedMedian(coldSamples),
				coldP95Milliseconds: roundedPercentile(coldSamples, 95),
				warmMilliseconds: roundedMedian(warmSamples),
				warmP95Milliseconds: roundedPercentile(warmSamples, 95),
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
						"median of module import, initialization, authoring, and first render in a fresh process",
					coldP95Milliseconds: "p95 of the same cold samples",
					warmMilliseconds: "median of authoring and render after one completed render",
					warmP95Milliseconds: "p95 of the same warm samples",
					peakRssBytes: "maximum resident set size of the isolated process",
					peakRssDeltaBytes: "peak RSS minus RSS before importing the renderer",
					packageBytes: "the library's own resolved package directory only",
					installBytes:
						"packageBytes plus every transitive runtime dependency, deduped by real path",
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

// Shared fixture: A4 page, 36pt margins, 10pt body text, 24pt heading, 80/220
// rows, identical text, and a 70pt right-aligned amount column with no other
// library expressing a gap the others don't.
async function createFlashPdfRenderer(fixture) {
	const [{ render }, { jsx, jsxs }] = await Promise.all([
		import("../dist/index.js"),
		import("../dist/jsx-runtime.js"),
	]);
	const row = { display: "flex", flexDirection: "row", fontSize: 10 };
	return () =>
		render(
			jsxs("main", {
				style: { fontSize: 10 },
				children:
					fixture === "invoice"
						? [
								jsx("h1", { style: { fontSize: 24 }, children: "Invoice" }),
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
								jsx("h1", { style: { fontSize: 24 }, children: "Quarterly report" }),
								...Array.from({ length: 220 }, (_, index) =>
									jsx("p", {
										children: `Section ${index + 1}. Revenue remained stable across the reporting period.`,
									}),
								),
							],
			}),
			{ pageFormat: "A4", margin: 36 },
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
			document.fontSize(24).text(fixture === "invoice" ? "Invoice" : "Quarterly report");
			document.fontSize(10);
			if (fixture === "invoice") {
				const left = document.page.margins.left;
				const amountWidth = 70;
				const nameWidth =
					document.page.width -
					document.page.margins.left -
					document.page.margins.right -
					amountWidth;
				const rowHeight = document.currentLineHeight(true);
				const bottom = document.page.height - document.page.margins.bottom;
				let y = document.y;
				for (let index = 0; index < 80; index++) {
					if (y + rowHeight > bottom) {
						document.addPage();
						y = document.page.margins.top;
					}
					// pdfkit has no flex layout: emulate the name/amount flex row with fixed x positions for a 70pt right-aligned amount column.
					document.text(`Consulting item ${index + 1}`, left, y, {
						width: nameWidth,
						lineBreak: false,
					});
					document.text("$100.00", left + nameWidth, y, {
						width: amountWidth,
						align: "right",
						lineBreak: false,
					});
					y += rowHeight;
				}
			} else {
				for (let index = 0; index < 220; index++) {
					document.text(
						`Section ${index + 1}. Revenue remained stable across the reporting period.`,
					);
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
		page: { padding: 36, fontSize: 10 },
		heading: { fontSize: 24 },
		row: { flexDirection: "row" },
		name: { flexGrow: 1 },
		amount: { width: 70, textAlign: "right" },
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
									{ key: index },
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
				{ style: { fontSize: "10pt" } },
				h(
					"h1",
					{ style: { fontSize: "24pt" } },
					fixture === "invoice" ? "Invoice" : "Quarterly report",
				),
				...(fixture === "invoice"
					? Array.from({ length: 80 }, (_, index) =>
							h(
								"div",
								{ key: index, style: { display: "flex" } },
								h("span", { style: { flex: 1 } }, `Consulting item ${index + 1}`),
								h("span", { style: { width: "70pt", textAlign: "right" } }, "$100.00"),
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
			// takumi's margin is CSS px at 96dpi: 48px == 36pt, matching the other libraries' margin.
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

function percentile(values, p) {
	const sorted = values.toSorted((left, right) => left - right);
	const index = Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1));
	return sorted[index];
}

function roundedPercentile(values, p) {
	return Number(percentile(values, p).toFixed(2));
}

function packageStats(library) {
	const root = packageRoot(library);
	const files =
		library === "flashpdf"
			? ["package.json", "README.md", "CHANGELOG.md", "LICENSE", "dist", "wasm"]
			: ["."];
	const packageBytes = files.reduce((sum, file) => sum + pathBytes(join(root, file)), 0);
	// FlashPDF has no runtime dependencies, so packageBytes and installBytes are equal; that's the point.
	const dependencyDirs = collectDependencyDirs(root);
	const installBytes = packageBytes + dependencyDirs.reduce((sum, dir) => sum + pathBytes(dir), 0);
	return {
		version: JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version,
		packageBytes,
		installBytes,
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

// Walks a package's declared `dependencies`, resolved relative to the package's own
// directory so pnpm's per-package node_modules (or a flat node_modules) both work,
// and recurses into each dependency's own dependencies. Dedupes by real path so a
// dependency reachable through multiple paths (or a cycle) is only counted once.
function collectDependencyDirs(root) {
	const seen = new Set([realpathSync(root)]);
	const dependencyDirs = [];
	const queue = [root];
	while (queue.length) {
		const dir = queue.shift();
		const manifest = join(dir, "package.json");
		if (!existsSync(manifest)) continue;
		const dependencies = JSON.parse(readFileSync(manifest, "utf8")).dependencies ?? {};
		for (const name of Object.keys(dependencies)) {
			let depDir;
			try {
				depDir = resolveDependencyDir(name, manifest);
			} catch {
				continue; // optional or unresolvable dependency
			}
			const real = realpathSync(depDir);
			if (seen.has(real)) continue;
			seen.add(real);
			dependencyDirs.push(depDir);
			queue.push(depDir);
		}
	}
	return dependencyDirs;
}

// Resolves a dependency's directory the way Node resolves modules: from the
// requiring package's own file, so each package sees its own nested/hoisted copy.
function resolveDependencyDir(name, fromManifest) {
	const require = createRequire(fromManifest);
	try {
		return dirname(require.resolve(`${name}/package.json`));
	} catch {
		// Some packages' "exports" map hides package.json (e.g. no "./package.json" entry).
		// Fall back to resolving the main entry and walking up to the matching manifest.
		let current = dirname(require.resolve(name));
		while (current !== dirname(current)) {
			const manifest = join(current, "package.json");
			if (existsSync(manifest) && JSON.parse(readFileSync(manifest, "utf8")).name === name)
				return current;
			current = dirname(current);
		}
		throw new Error(`cannot resolve package directory for ${name}`);
	}
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
