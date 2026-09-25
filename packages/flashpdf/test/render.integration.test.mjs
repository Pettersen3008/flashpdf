import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { inflateSync } from "node:zlib";

import { test } from "vitest";

import { PageNumber, render, stylesheet, TotalPages } from "../dist/index.js";
import { Fragment, jsx, jsxs } from "../dist/jsx-runtime.js";

/** Latin-1 view of the PDF with FlateDecode streams inflated so content operators are greppable. */
const string = (pdf) =>
	Buffer.from(pdf)
		.toString("latin1")
		.replace(/stream\r?\n([\s\S]*?)\r?\nendstream/g, (match, body) => {
			try {
				return `stream\n${inflateSync(Buffer.from(body, "latin1")).toString("latin1")}\nendstream`;
			} catch {
				return match;
			}
		});
const font = new Uint8Array(readFileSync(new URL("./fixtures/Abel-Regular.ttf", import.meta.url)));

test("given the package entry point, when importing it, then exposes only the renderer and stylesheet validator", async () => {
	const api = await import("../dist/index.js");
	assert.deepEqual(Object.keys(api).sort(), ["PageNumber", "TotalPages", "render", "stylesheet"]);
});

test("given the rendering entry point, when following its imports, then it has no build-tool dependencies", () => {
	const seen = new Set();
	const visit = (url) => {
		if (seen.has(url.href)) return;
		seen.add(url.href);
		for (const [, specifier] of readFileSync(url, "utf8").matchAll(/ from ["']([^"']+)["']/g)) {
			if (specifier.startsWith(".")) visit(new URL(specifier, url));
			else assert.notEqual(specifier, "lightningcss", `${url.pathname} imports ${specifier}`);
		}
	};
	visit(new URL("../dist/index.js", import.meta.url));
	assert.ok(seen.size > 2, [...seen].join(", "));
});

test("given native JSX components and fragments, when rendering, then preserves dynamic content", async () => {
	const Meta = ({ label, value }) =>
		jsxs("div", {
			style: { display: "flex", flexDirection: "row" },
			children: [
				jsx("span", { style: { width: "120pt" }, children: label }),
				jsx("span", { style: { flex: 1, textAlign: "right" }, children: value }),
			],
		});
	const document = jsxs("main", {
		children: [
			jsx("h1", { children: "Invoice" }),
			jsx(Meta, { label: "Number", value: 1001 }),
			jsx(Fragment, { children: ["Paid", false, null] }),
		],
	});
	const pdf = await render(document, { pageFormat: "A4", margin: 36 });
	assert.match(string(pdf), /\(Invoice\)/);
	assert.match(string(pdf), /\(1001\)/);
	assert.deepEqual(pdf, await render(document, { pageFormat: "A4", margin: 36 }));

	// A root fragment flattens to an array; the root check must look through it.
	const siblings = jsxs(Fragment, {
		children: [jsx("h1", { children: "One" }), jsx("p", { children: "Two" })],
	});
	assert.match(string(await render(siblings)), /\(Two\)/);
	await assert.rejects(render("bare text"), /expected element or props/);
});

test("given registered regular and bold fonts, when fontFamily selects them, then embeds both TTF files", async () => {
	const document = jsxs("main", {
		className: "invoice",
		children: [
			jsx("p", { children: "Regular" }),
			jsx("p", { style: { fontWeight: "bold" }, children: "Bold" }),
		],
	});
	const options = {
		fonts: [{ family: "Abel", regular: font, bold: font }],
		stylesheets: [".invoice { font-family: Abel }"],
	};
	const pdf = await render(document, options);
	assert.equal((string(pdf).match(/\/Subtype \/TrueType/g) ?? []).length, 2);
	await assert.rejects(
		render(document, {
			fonts: [{ family: "Abel", regular: font }],
			stylesheets: options.stylesheets,
		}),
		/font family has no bold face/,
	);
	await assert.rejects(
		render(jsx("main", { style: { fontFamily: "Missing" }, children: "x" })),
		/unregistered font family/,
	);
});

test("given CSS cascade variables and a box, when rendering, then emits painted native content", async () => {
	const css = stylesheet(
		`main.invoice { --ink: #123; color: var(--ink); } .card { padding: 4pt; background: #fff; border: 1pt solid #456; } .amount { color: #456; font-weight: bold; text-align: right; }`,
	);
	const document = jsx("main", {
		className: "invoice",
		children: jsx("div", {
			className: "card",
			children: jsxs("div", {
				style: { display: "flex", flexDirection: "row" },
				children: [
					jsx("span", { children: "Total" }),
					jsx("span", { className: "amount", children: "$100" }),
				],
			}),
		}),
	});
	const pdf = await render(document, { stylesheets: [css] });
	assert.match(string(pdf), /re\nf/);
	assert.match(string(pdf), /\(\$100\)/);
});

test("given a long native main with a page break rule, when rendering, then it paginates", async () => {
	const document = jsx("main", {
		children: Array.from({ length: 100 }, (_, index) =>
			jsx("p", { className: index === 50 ? "new-page" : undefined, children: `Line ${index}` }),
		),
	});
	const pdf = await render(document, { stylesheets: [".new-page { break-before: page }"] });
	const path = new URL("../../../dist/native-flow.pdf", import.meta.url);
	writeFileSync(path, pdf);
	const parsed = spawnSync(
		"cargo",
		[
			"run",
			"--locked",
			"-q",
			"-p",
			"flashpdf-core",
			"--example",
			"validate_pdf",
			"--",
			path.pathname,
			"2",
		],
		{ cwd: new URL("../../../", import.meta.url), encoding: "utf8" },
	);
	assert.equal(parsed.status, 0, parsed.stderr + parsed.stdout);
});

test("given a page footer, when rendering multiple pages, then repeats resolved page numbers", async () => {
	const document = jsx("main", {
		children: Array.from({ length: 100 }, (_, index) => jsx("p", { children: `Line ${index}` })),
	});
	const footer = jsxs("footer", {
		style: { fontSize: "9pt", textAlign: "center" },
		children: ["Page ", jsx(PageNumber, {}), " of ", jsx(TotalPages, {})],
	});
	const pdf = await render(document, { footer });
	assert.match(string(pdf), /\(Page 1 of 2\)/);
	assert.match(string(pdf), /\(Page 2 of 2\)/);
	await assert.rejects(
		render(jsx("main", { children: jsx(PageNumber, {}) })),
		/only valid in a footer/,
	);
});

test("given adjacent text and plain spans, when rendering, then keeps them in one inline run", async () => {
	const pdf = await render(
		jsx("main", {
			children: [jsx("span", { children: "Label:" }), " value"],
		}),
	);
	assert.match(string(pdf), /\(Label: value\)/);
});

test("given inline bold inside a paragraph, when rendering, then paints both fonts in one text object", async () => {
	const pdf = await render(
		jsxs("p", { children: ["Hello ", jsx("b", { children: "World" }), " again"] }),
	);
	const objects = string(pdf)
		.split("ET")
		.filter((object) => object.includes("Tj"));
	assert.equal(objects.length, 1, objects.join("\n"));
	assert.match(
		objects[0],
		/\/F1 12 Tf\n[\d.]+ [\d.]+ Td\n\(Hello \) Tj\n\/F2 12 Tf\n[\d.]+ 0 Td\n\(World\) Tj\n\/F1 12 Tf\n[\d.]+ 0 Td\n\( again\) Tj/,
	);
});

test("given a long margin-only wrapper, when rendering, then it paginates", async () => {
	const pdf = await render(
		jsx("main", {
			style: { marginTop: "12pt" },
			children: Array.from({ length: 100 }, (_, index) => jsx("p", { children: `Line ${index}` })),
		}),
	);
	assert.match(string(pdf), /\/Count 2/);
});

test("given per-edge borders and hr, when rendering, then emits native rules", async () => {
	const pdf = await render(
		jsx("main", {
			children: [
				jsx("div", { style: { borderBottom: "1pt solid red" }, children: "Total" }),
				jsx("hr", {}),
			],
		}),
	);
	assert.match(string(pdf), /1 0 0 rg/);
});

test("given an hr border, when rendering, then uses the author rule", async () => {
	const pdf = await render(jsx("hr", { style: { border: "2pt solid red" } }));
	assert.match(string(pdf), /1 0 0 rg/);
});

test("given non-native JSX, when rendering, then rejects it", async () => {
	await assert.rejects(
		render({ type: "table", props: {} }),
		/unsupported element <table>: tables are not supported yet/,
	);
});

test("given an unpaintable color deep in the tree, when rendering, then names the element and its ancestors", async () => {
	const document = jsx("main", {
		className: "invoice",
		children: jsx("div", {
			className: "row",
			children: jsx("span", {
				className: "total",
				style: { color: "oklch(50% 0.1 20)" },
				children: "x",
			}),
		}),
	});
	await assert.rejects(
		render(document),
		/unsupported color: oklch\(50% 0.1 20\) on <span\.total> in <div\.row> > <main\.invoice>/,
	);
	await assert.rejects(
		render(jsx("p", { className: "text-red-500", children: "x" }), {
			stylesheets: [".text-red-500 { color: oklch(63.7% 0.237 25.331) }"],
		}),
		/unsupported color: oklch\(63.7% 0.237 25.331\) on <p\.text-red-500>/,
	);
});

test("given unsupported layout styles, when rendering, then rejects instead of dropping them", async () => {
	for (const [style, message] of [
		[{ height: 40 }, /height \(block height comes from content\) on <main>/],
		[{ borderRadius: 4 }, /border-?[Rr]adius \(box corners are square\) on <main>/],
		[{ zoom: 2 }, /unsupported style property: zoom on <main>/],
		[{ display: "grid" }, /invalid display/],
		[{ width: "40pt" }, /width applies only to a flex row child on <main>/],
	])
		await assert.rejects(
			render(jsx("main", { style, children: "x" })),
			message,
			JSON.stringify(style),
		);
});

test("given two stylesheets of equal specificity, when rendering, then the later one wins", async () => {
	const pdf = await render(jsx("main", { className: "ink", children: "Total" }), {
		stylesheets: [".ink { color: #ff0000 }", ".ink { color: #0000ff }"],
	});
	assert.match(string(pdf), /0 0 1 rg/);
	assert.doesNotMatch(string(pdf), /1 0 0 rg/);
});
