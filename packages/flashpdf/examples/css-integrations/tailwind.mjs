import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { render, stylesheet } from "../../dist/index.js";
import { jsx } from "../../dist/jsx-runtime.js";

const root = dirname(dirname(dirname(dirname(fileURLToPath(import.meta.url)))));
const work = mkdtempSync(join(tmpdir(), "flashpdf-tailwind-"));
try {
	const output = join(work, "tailwind.css");
	execFileSync(
		"pnpm",
		[
			"exec",
			"tailwindcss",
			"-i",
			fileURLToPath(new URL("./tailwind.css", import.meta.url)),
			"-o",
			output,
			"--minify",
		],
		{ cwd: root, stdio: "pipe" },
	);
	const css = stylesheet(readFileSync(output, "utf8"));

	const h = (tag, className, children) => jsx(tag, { className, children });
	const row = (name, amount) =>
		h("tr", "odd:bg-gray-100", [
			h("td", "px-4 py-2 border-b border-gray-200", name),
			h("td", "px-4 py-2 border-b border-gray-200 text-right", amount),
		]);
	const invoice = h("main", "text-sm text-gray-600", [
		h("div", "flex", [
			h("div", "w-1/2", [
				h("h1", "text-lg font-bold", "Acme Supply Co."),
				h("p", undefined, "12 Harbour Street"),
			]),
			h("div", "w-1/2 text-right", [
				h("p", "font-semibold", "Invoice 2026-0042"),
				h("p", undefined, "Due 30 September 2026"),
			]),
		]),
		h("div", "mt-4 p-4 md:p-8 bg-gray-100 border border-gray-200", "Bill to: Globex Corporation"),
		h("table", "mt-4", [
			h("thead", undefined, [
				h("tr", undefined, [
					h("th", "w-1/2 px-4 py-2 border-b border-gray-200", "Item"),
					h("th", "w-32 px-4 py-2 border-b border-gray-200 text-right", "Amount"),
				]),
			]),
			h("tbody", undefined, [row("Widget", "$48.00"), row("Gadget", "$120.00")]),
		]),
		h("div", "flex flex-col gap-4 mt-4 text-right", [
			h("p", undefined, "Subtotal $168.00"),
			h("p", "font-bold", "Total $210.00"),
		]),
	]);
	const pdf = await render(invoice, { stylesheets: [css] });
	assert.equal(new TextDecoder().decode(pdf.subarray(0, 5)), "%PDF-");
	assert.deepEqual(pdf, await render(invoice, { stylesheets: [css] }));
	// Without the stylesheet the same tree draws no borders, fills, or spacing, so the bytes differ.
	assert.notDeepEqual(pdf, await render(invoice));
} finally {
	rmSync(work, { recursive: true, force: true });
}
