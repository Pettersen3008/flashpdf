import { statSync } from "node:fs";
import { performance } from "node:perf_hooks";

import { render } from "../dist/index.js";
import { jsx, jsxs } from "../dist/jsx-runtime.js";

const css = `.item { color: #222; font-size: 10pt; display: flex; flex-direction: row } .amount { width: 80pt; color: #456; text-align: right }`;
const invoice = jsxs("main", {
	children: [
		jsx("h1", { children: "Invoice" }),
		...Array.from({ length: 100 }, (_, index) =>
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
global.gc?.();
const before = process.memoryUsage().heapUsed;
const start = performance.now();
const pdf = await render(invoice, { stylesheets: [css] });
const milliseconds = performance.now() - start;
global.gc?.();
console.log(
	JSON.stringify({
		invoiceRows: 100,
		renderMilliseconds: Number(milliseconds.toFixed(2)),
		pdfBytes: pdf.length,
		jsHeapDeltaBytes: process.memoryUsage().heapUsed - before,
		wasmBytes: statSync(new URL("../wasm/flashpdf_wasm_bg.wasm", import.meta.url)).size,
	}),
);
