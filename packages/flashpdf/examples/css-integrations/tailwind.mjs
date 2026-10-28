import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { render, stylesheet } from "../../dist/index.js";
import { jsx } from "../../dist/jsx-runtime.js";

const root = fileURLToPath(new URL("../../../../", import.meta.url));
const work = mkdtempSync(join(tmpdir(), "flashpdf-tailwind-"));
try {
	const build = (input) => {
		const output = join(work, input);
		execFileSync(
			"pnpm",
			[
				"exec",
				"tailwindcss",
				"-i",
				fileURLToPath(new URL(`./${input}`, import.meta.url)),
				"-o",
				output,
				"--minify",
			],
			{ cwd: root, stdio: "pipe" },
		);
		return stylesheet(readFileSync(output, "utf8"));
	};
	const css = build("tailwind.css");
	// Tailwind's own layered import without preflight: theme variables on :root, utilities in
	// @layer, nested @media, @supports fallbacks, and @property defaults.
	const layered = build("tailwind-layers.css");

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
	assert.deepEqual(pdf, await render(invoice, { stylesheets: [layered] }));
	// Without the stylesheet the same tree draws no borders, fills, or spacing, so the bytes differ.
	assert.notDeepEqual(pdf, await render(invoice));

	// The `tw` prop needs no build: the same utilities as classes and as `tw` give the same bytes.
	const summary = (prop) => {
		const e = (tag, utilities, children) => jsx(tag, { [prop]: utilities, children });
		return e("main", "text-sm text-gray-600", [
			e("div", "flex", [
				e("div", "w-1/2", e("h1", "text-lg font-bold", "Acme Supply Co.")),
				e("p", "w-1/2 text-right font-semibold leading-6", "Invoice 2026-0042"),
			]),
			e("div", "mt-4 p-4 bg-gray-100 border border-gray-200", "Bill to: Globex Corporation"),
			e("table", "mt-4", [
				e("tr", undefined, [
					e("td", "w-32 px-4 py-2 border-b border-gray-200", "Widget"),
					e("td", "px-4 py-2 border-b border-gray-200 text-right", "$48.00"),
				]),
			]),
			e("p", "mt-4 text-right underline", "Total $48.00"),
		]);
	};
	assert.deepEqual(
		await render(summary("tw")),
		await render(summary("className"), { stylesheets: [layered] }),
	);
	// Given conflicting utilities, the built CSS and tw must agree in either token order.
	const cases = [
		"p-10 p-2 px-8 pl-1 [padding-top:3px]",
		"m-4 mx-2 ml-1 mt-1",
		"rounded-full bg-gray-100 w-32 h-8",
		"rounded-lg rounded-t-sm rounded-tl-none bg-blue-100 p-4",
		"border-4 border-2 border-dashed border-red-500 border-blue-500 p-4",
		"border-2 border-l-8 border-gray-200 p-4",
		"text-sm text-lg leading-6 font-bold font-normal tracking-tight tracking-wide",
		"text-[min(2rem,3rem)] text-[length:clamp(9pt,12pt,14pt)] text-[#0a0]",
		"font-[Arial] text-[color:rgb(0_128_0)]",
		"font-[calc(600+100)] p-[calc(1rem-2px)]",
		"[--my_color:#0a0] text-[var(--my_color)]",
		"[--size:2px] p-[calc(1rem-var(--size))]",
		"p-[calc(1rem--2px)]",
		'font-["foo:bar",Arial]',
		"w-8 size-4 h-6 bg-neutral-100",
		"flex flex-wrap gap-2 gap-x-4 gap-y-1 items-center content-between h-20",
		"whitespace-pre-wrap whitespace-normal underline no-underline uppercase normal-case",
	];
	const input = join(work, "tw.css");
	const output = join(work, "tw-built.css");
	writeFileSync(
		input,
		`@import "${join(root, "node_modules/tailwindcss/theme.css")}" theme(inline);
@import "${join(root, "node_modules/tailwindcss/utilities.css")}" source(none);
@source inline('${[...cases, "text-lg leading-6 border-dashed border-2"].join(" ")}');`,
	);
	execFileSync("pnpm", ["exec", "tailwindcss", "-i", input, "-o", output, "--minify"], {
		cwd: root,
		stdio: "pipe",
	});
	const fixtureCss = stylesheet(readFileSync(output, "utf8"));
	for (const utilities of cases) {
		const tree = (prop, names) => jsx("div", { [prop]: names, children: "x" });
		const expected = await render(tree("className", utilities), { stylesheets: [fixtureCss] });
		assert.deepEqual(await render(tree("tw", utilities)), expected, utilities);
		assert.deepEqual(
			await render(tree("tw", utilities.split(" ").reverse().join(" "))),
			expected,
			`reversed ${utilities}`,
		);
	}
	const nested = (prop) =>
		jsx("div", {
			[prop]: "leading-6 border-dashed",
			children: jsx("p", { [prop]: "text-lg border-2", children: "Local utility defaults" }),
		});
	assert.deepEqual(
		await render(nested("tw")),
		await render(nested("className"), { stylesheets: [fixtureCss] }),
	);
} finally {
	rmSync(work, { recursive: true, force: true });
}
