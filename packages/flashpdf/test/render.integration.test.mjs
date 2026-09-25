import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import zlib, { inflateSync } from "node:zlib";

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
	assert.equal((string(pdf).match(/\/Subtype \/Type0/g) ?? []).length, 2);
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

test("given text outside WinAnsi in an embedded font, when rendering, then emits an Identity-H subset with a ToUnicode map that extracts and validates", async () => {
	// Norwegian plus Polish ł: U+0142 is outside WinAnsi but in Abel.
	const text = "Blåbærsyltetøy på Ærø – łaskawy 3 €";
	const options = { fonts: [{ family: "Abel", regular: font }] };
	const pdf = await render(
		jsx("main", { style: { fontFamily: "Abel" }, children: jsx("p", { children: text }) }),
		options,
	);
	assert.match(string(pdf), /\/Subtype \/Type0/);
	assert.match(string(pdf), /\/Encoding \/Identity-H/);
	assert.match(string(pdf), /\/Subtype \/CIDFontType2/);
	assert.match(string(pdf), /\/ToUnicode \d+ 0 R/);
	assert.match(string(pdf), /\/BaseFont \/[A-Z]{6}\+Abel-Regular/);
	const path = new URL("../../../dist/unicode-abel.pdf", import.meta.url);
	writeFileSync(path, pdf);
	const cargo = ["run", "--locked", "-q", "-p", "flashpdf-core", "--example", "validate_pdf"];
	const parsed = spawnSync("cargo", [...cargo, "--", path.pathname, "1"], {
		cwd: new URL("../../../", import.meta.url),
		encoding: "utf8",
	});
	assert.equal(parsed.status, 0, parsed.stderr + parsed.stdout);
	assert.equal(parsed.stdout, text);
	const qpdf = spawnSync("qpdf", ["--check", path.pathname], { encoding: "utf8" });
	if (qpdf.error?.code !== "ENOENT") assert.equal(qpdf.status, 0, qpdf.stdout + qpdf.stderr);
	await assert.rejects(
		render(
			jsx("main", {
				style: { fontFamily: "Abel" },
				children: jsx("p", { className: "cjk", children: "中" }),
			}),
			options,
		),
		/the font has no glyph for "中" \(U\+4E2D\) on <p\.cjk> in <main>/,
	);
	await assert.rejects(
		render(jsx("p", { children: "中" })),
		/Helvetica has no glyph for "中" \(U\+4E2D\); register a font that has it on <p>/,
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
		render({ type: "ul", props: {} }),
		/unsupported element <ul>: lists are not supported yet/,
	);
});

test("given a table that spans two pages, when rendering, then the header row repeats on the second page", async () => {
	const items = Array.from({ length: 70 }, (_, index) => `Item ${index + 1}`);
	const pdf = await render(
		jsx("main", {
			children: jsxs("table", {
				children: [
					jsx("thead", {
						children: jsxs("tr", {
							children: [
								jsx("th", { children: "Name" }),
								jsx("th", { style: { width: "60pt", textAlign: "right" }, children: "Qty" }),
							],
						}),
					}),
					jsx("tbody", {
						children: items.map((item, index) =>
							jsxs("tr", {
								children: [
									jsx("td", { children: item }),
									jsx("td", { style: { textAlign: "right" }, children: index + 1 }),
								],
							}),
						),
					}),
				],
			}),
		}),
	);
	const path = new URL("../../../dist/table-pages.pdf", import.meta.url);
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
	const pages = parsed.stdout.split("\f");
	assert.equal(pages.length, 2);
	const body = pages.flatMap((page) => {
		assert.match(page, /^Name\nQty\n/);
		return page
			.split("\n")
			.slice(2)
			.filter((line) => line.startsWith("Item"));
	});
	assert.deepEqual(body, items);
	await assert.rejects(
		render(jsx("main", { children: jsx("tr", { children: jsx("td", { children: "x" }) }) })),
		/<tr> belongs in a <table> on <tr> in <main>/,
	);
	await assert.rejects(
		render(
			jsx("table", { children: jsx("tr", { children: jsx("td", { colSpan: 2, children: "x" }) }) }),
		),
		/colSpan and rowSpan are not supported yet; use one cell per column on <td> in <tr> > <table>/,
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
		[{ height: 40 }, /height applies only to <img> \(block height comes from content\) on <main>/],
		[{ textDecoration: "overline" }, /overline is not supported on <main>/],
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

/** A PNG built from raw scanlines; `type` 2 is RGB and 6 is RGBA. */
function png(width, height, type, scanlines) {
	const chunk = (kind, data) => {
		const body = Buffer.concat([Buffer.from(kind), data]);
		const crc = Buffer.alloc(4);
		crc.writeUInt32BE(zlib.crc32(body));
		const length = Buffer.alloc(4);
		length.writeUInt32BE(data.length);
		return Buffer.concat([length, body, crc]);
	};
	const header = Buffer.alloc(13);
	header.writeUInt32BE(width, 0);
	header.writeUInt32BE(height, 4);
	header.set([8, type, 0, 0, 0], 8);
	return new Uint8Array(
		Buffer.concat([
			Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
			chunk("IHDR", header),
			chunk("IDAT", zlib.deflateSync(Buffer.from(scanlines))),
			chunk("IEND", Buffer.alloc(0)),
		]),
	);
}

test("given an invoice with a PNG logo on every row and a link, when rendering, then embeds one XObject and annotates the link", async () => {
	const logo = png(2, 1, 2, [0, 255, 0, 0, 0, 0, 255]);
	const stamp = png(1, 1, 6, [0, 0, 0, 0, 128]);
	const document = jsxs("main", {
		children: [
			jsx("a", {
				href: "https://example.com/",
				children: jsx("img", { src: logo, alt: "Acme", style: { width: 60 } }),
			}),
			jsxs("p", {
				children: [
					"Pay at ",
					jsx("a", { href: "https://example.com/pay?id=42", children: "the portal" }),
				],
			}),
			jsxs("table", {
				children: [
					jsx("thead", {
						children: jsxs("tr", {
							children: [jsx("th", { children: "Item" }), jsx("th", { children: "Seal" })],
						}),
					}),
					jsx("tbody", {
						children: Array.from({ length: 3 }, (_, index) =>
							jsxs(
								"tr",
								{
									children: [
										jsx("td", { children: `Line ${index}` }),
										jsx("td", { children: jsx("img", { src: stamp, style: { height: "9pt" } }) }),
									],
								},
								index,
							),
						),
					}),
				],
			}),
		],
	});
	const pdf = await render(document);
	assert.deepEqual(pdf, await render(document));
	const text = string(pdf);
	assert.equal(text.match(/\/Subtype \/Image/g).length, 3, "logo, stamp, and the stamp's SMask");
	assert.equal(text.match(/\/Im1 Do/g).length, 1);
	assert.equal(text.match(/\/Im2 Do/g).length, 3);
	assert.match(text, /\/Predictor 15/);
	assert.match(text, /\/SMask \d+ 0 R/);
	assert.match(text, /\/Subtype \/Link[\s\S]*?\/URI \(https:\/\/example\.com\/\)/);
	assert.match(text, /\/URI \(https:\/\/example\.com\/pay\?id=42\)/);
	assert.equal(text.match(/\/Subtype \/Link/g).length, 2);
	assert.match(text, /0 0 0\.93333334 rg/, "link colour #0000EE");
	if (spawnSync("qpdf", ["--version"]).status === 0) {
		const path = new URL("../../../target/render-images.pdf", import.meta.url);
		writeFileSync(path, pdf);
		const check = spawnSync("qpdf", ["--check", path.pathname], { encoding: "utf8" });
		assert.equal(check.status, 0, check.stdout + check.stderr);
	}
	await assert.rejects(
		render(jsx("main", { children: jsx("img", { src: logo, style: { width: 600 } }) })),
		/the image is wider than its container; reduce width or height on <img> in <main>/,
	);
	await assert.rejects(
		render(jsx("main", { children: jsx("img", { src: logo, style: { width: 10, height: 900 } }) })),
		/PageOverflow: the image is taller than the page; reduce its height on <img> in <main>/,
	);
	await assert.rejects(
		render(jsx("main", { children: jsx("img", { src: new Uint8Array([1, 2, 3]) }) })),
		/unsupported image format; pass PNG or JPEG bytes on <img> in <main>/,
	);
});

test("given two stylesheets of equal specificity, when rendering, then the later one wins", async () => {
	const pdf = await render(jsx("main", { className: "ink", children: "Total" }), {
		stylesheets: [".ink { color: #ff0000 }", ".ink { color: #0000ff }"],
	});
	assert.match(string(pdf), /0 0 1 rg/);
	assert.doesNotMatch(string(pdf), /1 0 0 rg/);
});
