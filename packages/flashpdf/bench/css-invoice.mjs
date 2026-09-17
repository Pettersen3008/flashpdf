import { statSync } from "node:fs";
import { performance } from "node:perf_hooks";

import { render } from "../dist/index.js";
import { jsx, jsxs } from "../dist/jsx-runtime.js";

const css = `.item { color: #222; font-size: 10pt; display: flex; flex-direction: row } .amount { width: 80pt; color: #456; text-align: right }`;
const runs = Number(process.env.FLASHPDF_BENCH_RUNS ?? 20);
if (!Number.isInteger(runs) || runs < 1) throw new Error("invalid FLASHPDF_BENCH_RUNS");

function invoice(rows) {
	return jsxs("main", {
		children: [
			jsx("h1", { children: "Invoice" }),
			...Array.from({ length: rows }, (_, index) =>
				jsxs("div", {
					className: "item",
					children: [
						jsx("span", { children: `Item ${index + 1}` }),
						jsx("span", { className: "amount", children: "$100.00" }),
					],
				}),
			),
		],
	});
}

function percentile(values, fraction) {
	const sorted = values.toSorted((a, b) => a - b);
	return sorted[Math.min(Math.ceil(sorted.length * fraction) - 1, sorted.length - 1)];
}

const results = [];
for (const rows of [10, 100, 1000]) {
	const document = invoice(rows);
	for (let warmup = 0; warmup < 3; warmup++) await render(document, { stylesheets: [css] });
	const milliseconds = [];
	const heap = [];
	let pdf;
	for (let run = 0; run < runs; run++) {
		global.gc?.();
		const before = process.memoryUsage().heapUsed;
		const start = performance.now();
		pdf = await render(document, { stylesheets: [css] });
		milliseconds.push(performance.now() - start);
		heap.push(process.memoryUsage().heapUsed - before);
	}
	results.push({
		invoiceRows: rows,
		medianMilliseconds: Number(percentile(milliseconds, 0.5).toFixed(2)),
		p95Milliseconds: Number(percentile(milliseconds, 0.95).toFixed(2)),
		medianHeapDeltaBytes: percentile(heap, 0.5),
		pdfBytes: pdf.length,
	});
}

console.log(
	JSON.stringify({
		runs,
		wasmBytes: statSync(new URL("../wasm/flashpdf_wasm_bg.wasm", import.meta.url)).size,
		results,
	}),
);
