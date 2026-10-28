import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { inflateSync } from "node:zlib";

import { test } from "vitest";

import { render, PageNumber } from "../dist/index.js";
import { jsx } from "../dist/jsx-runtime.js";
import { cssInvoice } from "../examples/css-invoice.tsx";
import { nativeInvoice } from "../examples/native-invoice.tsx";

const font = new Uint8Array(readFileSync(new URL("./fixtures/Abel-Regular.ttf", import.meta.url)));
const options = {
	pdfua: true,
	fonts: [{ family: "UA", regular: font, bold: font }],
	metadata: { title: "Accessible invoice", language: "en-US" },
};
const out = new URL("../../../dist/pdfua/", import.meta.url);
const unpack = (pdf) =>
	Buffer.from(pdf)
		.toString("latin1")
		.replace(/stream\r?\n([\s\S]*?)\r?\nendstream/g, (match, body) => {
			try {
				return inflateSync(Buffer.from(body, "latin1")).toString("latin1");
			} catch {
				return match;
			}
		});

test("given PDF/UA options and invoice examples, when rendering, then emits semantic tags and accepts PDF/A-3b and PDF/A-2u", async () => {
	mkdirSync(out, { recursive: true });
	const data = {
		number: "2026-001",
		issued: "2026-10-01",
		due: "2026-10-31",
		currency: "USD",
		taxRate: 0.2,
		seller: { name: "Seller", address: "One Street" },
		customer: { name: "Buyer", address: "Two Street" },
		items: Array.from({ length: 80 }, (_, i) => ({
			description: `Item ${i + 1}`,
			quantity: 1,
			unitPrice: 25,
		})),
	};
	const invoices = [
		["native-invoice", await nativeInvoice(data, options)],
		[
			"css-invoice",
			await cssInvoice(
				{
					title: "Invoice",
					number: data.number,
					dueDate: data.due,
					items: data.items.map((item, i) => ({
						id: `${i}`,
						name: item.description,
						total: "$25.00",
					})),
				},
				options,
			),
		],
	];
	const rows = Array.from(
		{ length: 60 },
		(_, i) => `<tr><th scope="row">Item ${i + 1}</th><td>${i + 1}</td></tr>`,
	).join("");
	const report = `<main style="font-family:UA"><h1>Report</h1><p>Read <a href="https://example.com">the results</a> in <span lang="nb">norsk</span>.</p><h2>Summary</h2><div style="position:relative;width:200pt;height:20pt">Flow<div style="position:absolute;left:100pt;top:0">Positioned</div></div><div style="position:fixed;left:0;bottom:0">Repeated mark</div><ul><li>First<ul><li>Nested</li></ul></li><li>Second</li></ul><p>${"One paragraph across several pages. ".repeat(180)}</p><table><thead><tr><th>Name</th><th>Count</th></tr></thead><tbody>${rows}</tbody><tfoot><tr><th>Total</th><td>1830</td></tr></tfoot></table></main>`;
	const footer = jsx("footer", {
		style: { fontFamily: "UA" },
		children: ["Page ", jsx(PageNumber, {})],
	});
	const plain = await render(report, { ...options, footer });
	assert.deepEqual(plain, await render(report, { ...options, footer }));
	const combined = await render(report, {
		...options,
		pdfa: "3b",
		footer,
		attachments: [
			{
				name: "invoice.xml",
				data: new TextEncoder().encode("<Invoice/>"),
				mimeType: "application/xml",
				relationship: "Data",
			},
		],
	});
	const unicode = await render(report, { ...options, pdfa: "2u", footer });
	for (const [name, bytes] of [
		...invoices,
		["report", plain],
		["pdfa3b", combined],
		["pdfa2u", unicode],
	]) {
		const text = unpack(bytes);
		assert.match(text, /pdfuaid:part>1</);
		assert.match(text, /\/DisplayDocTitle true/);
		assert.match(text, /\/S\s*\/H1/);
		assert.match(text, /\/Scope\s*\/Column/);
		writeFileSync(new URL(`${name}.pdf`, out), bytes);
	}
	assert.match(unpack(plain), /\/Type \/Pagination/);
	assert.match(unpack(plain), /\/Type\s*\/OBJR/);
	assert.match(unpack(plain), /\/Lang\s*\(nb\)/);
});

test("given invalid PDF/UA inputs, when rendering, then rejects missing metadata fonts image alt and heading levels", async () => {
	mkdirSync(out, { recursive: true });
	const text = jsx("p", { style: { fontFamily: "UA" }, children: "Text" });
	await assert.rejects(
		render(text, { ...options, metadata: { language: "en" } }),
		/requires metadata.title/,
	);
	await assert.rejects(
		render(text, { ...options, metadata: { title: "Title" } }),
		/requires metadata.language/,
	);
	await assert.rejects(render(jsx("p", { children: "Text" }), options), /PDF\/UA font/);
	const image = new TextEncoder().encode(
		'<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="red"/></svg>',
	);
	await assert.rejects(render(jsx("img", { src: image }), options), /requires explicit alt/);
	const figure = await render(jsx("img", { src: image, alt: "Red square" }), options);
	writeFileSync(new URL("figure.pdf", out), figure);
	assert.match(unpack(figure), /\/Alt\s*\(Red square\)/);
	const decorative = await render(jsx("img", { src: image, alt: "" }), options);
	writeFileSync(new URL("decorative.pdf", out), decorative);
	assert.doesNotMatch(unpack(decorative), /\/S\s*\/Figure/);
	await assert.rejects(
		render(jsx("h2", { style: { fontFamily: "UA" }, children: "Skipped" }), options),
		/cannot skip a level/,
	);
	await assert.rejects(render(text, { ...options, pdfua: "yes" }), /invalid pdfua/);
	await assert.rejects(
		render(jsx("span", { lang: "bad_language", children: "Text" }), options),
		/invalid lang/,
	);
	await assert.rejects(
		render(jsx("th", { scope: "invalid", children: "Header" }), options),
		/scope must be/,
	);
});

test("given layered mixed colors and painted transformed links in PDF/UA, when rendering, then preserves effects and semantic annotations", async () => {
	const source =
		'<main style="font-family:UA"><h1>Combined</h1><section style="display:grid;grid-template-columns:200pt 1fr"><div class="card"><a href="https://example.com">Linked text</a><div style="position:absolute;left:10pt;top:2pt">Positioned</div></div><p>Second cell</p></section></main>';
	const settings = {
		...options,
		pdfa: "2u",
		stylesheets: [
			"@layer theme, utilities; @layer theme { .card { color:red } } @layer utilities { .card { position:relative; width:200pt; height:30pt; transform:translate(2pt, 3pt); transform-origin:0 0; clip-path:inset(0 round 2pt); overflow:hidden; color:color-mix(in oklab, red, blue); box-shadow:2pt 3pt #0008; background-image:linear-gradient(30deg, white, color-mix(in srgb, red, blue)) } }",
		],
	};
	const pdf = await render(source, settings);
	assert.deepEqual(pdf, await render(source, settings));
	const text = unpack(pdf);
	assert.match(text, /1 0 0 1 2 -3 cm/);
	assert.match(text, /\/ShadingType 2/);
	assert.match(text, /\/Type\s*\/OBJR/);
	assert.match(text, /\/S\s*\/Link/);
	assert.match(text, /pdfuaid:part>1</);
	mkdirSync(out, { recursive: true });
	writeFileSync(new URL("combined-effects.pdf", out), pdf);
});
