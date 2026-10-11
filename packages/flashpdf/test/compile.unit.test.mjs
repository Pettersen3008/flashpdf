import assert from "node:assert/strict";

import { jsx, jsxs } from "react/jsx-runtime";
import { test } from "vitest";

import { compile } from "../dist/compile.js";
import { resolveStyles } from "../dist/css.js";
import { helvetica } from "../dist/style.js";
import { resolveTree } from "../dist/tree.js";

/** Records protocol calls instead of encoding them. */
async function compiled(element, fonts = new Map([["Helvetica", helvetica]])) {
	const calls = [];
	const writer = new Proxy(
		{},
		{
			get:
				(_, method) =>
				(...args) =>
					calls.push([method, ...args]),
		},
	);
	compile(resolveStyles(await resolveTree(element)), writer, fonts);
	return calls;
}

const run = (font, text, extra = {}) => ({
	font,
	size: 12,
	lineHeight: 0,
	color: [0, 0, 0],
	text,
	break: false,
	underline: false,
	lineThrough: false,
	decorationColor: undefined,
	link: undefined,
	oblique: false,
	letterSpacing: 0,
	wordSpacing: 0,
	transform: "none",
	...extra,
});

test("given text with an inline bold child, when compiling, then emits one paragraph with two runs", async () => {
	const calls = await compiled(
		jsxs("p", { children: ["Hello ", jsx("b", { children: "World" })] }),
	);
	assert.deepEqual(calls, [["paragraph", [run(0, "Hello "), run(1, "World")], "left"]]);
});

test("given a br between text, when compiling, then flags a hard break on the preceding run", async () => {
	const calls = await compiled(
		jsxs("div", { children: ["one", jsx("br", {}), jsx("br", {}), "two"] }),
	);
	assert.deepEqual(calls, [
		[
			"paragraph",
			[run(0, "one", { break: true }), run(0, "", { break: true }), run(0, "two")],
			"left",
		],
	]);
});

test("given a padded span in a block, when compiling, then it stays a box beside the paragraph", async () => {
	const calls = await compiled(
		jsxs("div", {
			children: [
				"Total",
				jsx("span", { style: { padding: 2, fontWeight: "bold" }, children: "$1" }),
			],
		}),
	);
	assert.deepEqual(
		calls.map(([method]) => method),
		["paragraph", "boxStart", "paragraph", "boxEnd"],
	);
	assert.deepEqual(calls[2][1], [run(1, "$1")]);
});

test("given a padded span inside a paragraph, when compiling, then rejects with the element path", async () => {
	await assert.rejects(
		compiled(jsx("p", { children: jsx("span", { style: { padding: 2 }, children: "x" }) })),
		/<p> accepts only text, <br>, and <span>, <b>, <strong>, <em>, <i>, <cite>, <a> without box styles on <p>/,
	);
});

test("given a link inside a paragraph, when compiling, then its runs carry the href, blue, and underline", async () => {
	const calls = await compiled(
		jsxs("p", {
			children: [
				"See ",
				jsx("a", { href: "https://example.com/a b", children: "the portal" }),
				jsx("span", { style: { textDecoration: "line-through" }, children: " old" }),
			],
		}),
	);
	assert.deepEqual(calls, [
		[
			"paragraph",
			[
				run(0, "See "),
				run(0, "the portal", {
					color: [0, 0, 238],
					underline: true,
					link: "https://example.com/a%20b",
				}),
				run(0, " old", { lineThrough: true }),
			],
			"left",
		],
	]);
});

test("given a link around an image, when compiling, then emits the image record with the href and alt", async () => {
	const src = new Uint8Array([1, 2, 3]);
	const calls = await compiled(
		jsx("a", {
			href: "mailto:billing@example.com",
			children: jsx("img", { src, alt: "Logo", style: { width: "50%", height: "16px" } }),
		}),
	);
	assert.deepEqual(calls, [
		["image", src, { kind: 2, value: 50 }, 12, "Logo", "mailto:billing@example.com"],
	]);
});

test("given a table with thead and tbody, when compiling, then emits the container with header count 1 and one row per tr", async () => {
	const cell = (tag, text, props = {}) => jsx(tag, { ...props, children: text });
	const calls = await compiled(
		jsxs("table", {
			children: [
				jsx("thead", {
					children: jsxs("tr", {
						children: [cell("th", "Item"), cell("th", "Amount", { style: { width: "80pt" } })],
					}),
				}),
				jsx("tbody", {
					children: jsxs("tr", { children: [cell("td", "Coffee"), cell("td", "$4")] }),
				}),
			],
		}),
	);
	const columns = [
		{ kind: 1, value: 1 },
		{ kind: 0, value: 80 },
	];
	const row = (font, ...texts) => [
		["rowStart", columns],
		...texts.flatMap((text) => [
			["stackStart", 0],
			["paragraph", [run(font, text)], "left"],
			["stackEnd"],
		]),
		["rowEnd"],
	];
	assert.deepEqual(calls, [
		["tableStart", 1, 0, { kind: 1, value: 1 }],
		...row(1, "Item", "Amount"),
		...row(0, "Coffee", "$4"),
		["tableEnd"],
	]);
});

test("given rows with different cell counts, when compiling, then rejects naming the tr path", async () => {
	await assert.rejects(
		compiled(
			jsxs("table", {
				children: [
					jsxs("tr", { children: [jsx("td", { children: "a" }), jsx("td", { children: "b" })] }),
					jsx("tr", { className: "short", children: jsx("td", { children: "a" }) }),
				],
			}),
		),
		/^Error: <tr> has 1 cells but the table has 2 columns on <tr\.short> in <table>$/,
	);
});

test("given a td outside a table row, when compiling, then rejects naming it", async () => {
	await assert.rejects(
		compiled(jsx("div", { children: jsx("td", { children: "x" }) })),
		/^Error: <td> belongs in a <tr> inside a <table> on <td> in <div>$/,
	);
});

test("given an hr with break-before: page, when compiling, then breaks the page before the rule", async () => {
	const calls = await compiled(jsx("hr", { style: { breakBefore: "page" } }));
	assert.deepEqual(
		calls.map(([method]) => method),
		["pageBreak", "boxStart", "boxEnd"],
	);
});

test("given width on an hr or padding on a br, when compiling, then rejects instead of dropping it", async () => {
	await assert.rejects(
		compiled(jsx("hr", { style: { width: "50%" } })),
		/width applies only to a flex row child on <hr>$/,
	);
	await assert.rejects(
		compiled(jsxs("p", { children: ["a", jsx("br", { style: { padding: 10 } }), "b"] })),
		/<br> takes no paddingTop; style the text around it on <br> in <p>$/,
	);
});

test("given a flex row with a 12pt gap, when compiling, then a fixed 12pt empty column separates its items", async () => {
	const calls = await compiled(
		jsxs("div", {
			style: { display: "flex", gap: "12pt" },
			children: [jsx("div", { children: "a" }), jsx("div", { children: "b" })],
		}),
	);
	const cell = (text) => [["stackStart", 0], ["paragraph", [run(0, text)], "left"], ["stackEnd"]];
	assert.deepEqual(calls, [
		[
			"rowStart",
			[
				{ kind: 1, value: 1 },
				{ kind: 0, value: 12 },
				{ kind: 1, value: 1 },
			],
		],
		...cell("a"),
		["spacer", 0],
		...cell("b"),
		["rowEnd"],
	]);
});

test("given whitespace-only text between flex items and column children, when compiling, then it adds no column, gap, or line", async () => {
	const calls = await compiled(
		jsxs("main", {
			style: { display: "flex", flexDirection: "column", gap: "10pt" },
			children: [
				jsxs("div", {
					style: { display: "flex" },
					children: [jsx("span", { children: "a" }), " ", jsx("span", { children: "b" })],
				}),
				" ",
				jsxs("div", {
					children: [jsx("span", { children: "c" }), " ", jsx("b", { children: "d" })],
				}),
			],
		}),
	);
	assert.deepEqual(calls, [
		[
			"rowStart",
			[
				{ kind: 1, value: 1 },
				{ kind: 1, value: 1 },
			],
		],
		["paragraph", [run(0, "a")], "left"],
		["paragraph", [run(0, "b")], "left"],
		["rowEnd"],
		["spacer", 10],
		["paragraph", [run(0, "c "), run(1, "d")], "left"],
	]);
});

test("given a unitless line-height on a block, when a child changes font size, then each run scales the factor by its own size", async () => {
	const calls = await compiled(
		jsx("p", {
			style: { lineHeight: 1.5 },
			children: ["a", jsx("span", { style: { fontSize: "20pt" }, children: "b" })],
		}),
	);
	assert.deepEqual(calls, [
		[
			"paragraph",
			[run(0, "a", { lineHeight: 18 }), run(0, "b", { size: 20, lineHeight: 30 })],
			"left",
		],
	]);
});

test("given em, i, and cite, when compiling, then they default to the Helvetica-Oblique slot", async () => {
	const calls = await compiled(
		jsxs("p", {
			children: [
				jsx("em", { children: "a" }),
				jsx("i", { children: "b" }),
				jsx("cite", { children: "c" }),
			],
		}),
	);
	assert.deepEqual(calls, [["paragraph", [run(2, "abc")], "left"]]);
});

test("given bold italic text in a family with only regular and bold faces, when compiling, then the run slants the bold face", async () => {
	const fonts = new Map([
		["Helvetica", helvetica],
		["Abel", { regular: 4, bold: 5 }],
	]);
	const calls = await compiled(
		jsx("p", {
			style: { fontFamily: "Abel", fontStyle: "italic", fontWeight: "bold" },
			children: "x",
		}),
		fonts,
	);
	assert.deepEqual(calls, [["paragraph", [run(5, "x", { oblique: true })], "left"]]);
});

test("given capitalize over a word split across runs, when compiling, then only word starts are uppercased", async () => {
	const calls = await compiled(
		jsxs("p", {
			style: { textTransform: "capitalize" },
			children: ["don't stop hel", jsx("b", { children: "lo" }), " e-mail"],
		}),
	);
	const capitalize = { transform: "capitalize" };
	assert.deepEqual(calls, [
		[
			"paragraph",
			[
				run(0, "Don't Stop Hel", capitalize),
				run(1, "lo", capitalize),
				run(0, " E-Mail", capitalize),
			],
			"left",
		],
	]);
});

test("given letter-spacing in em on a parent, when a child changes font size, then the child inherits the parent's points", async () => {
	const calls = await compiled(
		jsx("p", {
			style: { letterSpacing: "0.5em", wordSpacing: "-1pt" },
			children: jsx("span", { style: { fontSize: 24 }, children: "x" }),
		}),
	);
	assert.deepEqual(calls, [
		["paragraph", [run(0, "x", { size: 24, letterSpacing: 6, wordSpacing: -1 })], "left"],
	]);
});

test("given a text-decoration shorthand without a colour inside one with a colour, when compiling, then it resets the decoration colour", async () => {
	const calls = await compiled(
		jsxs("p", {
			style: { textDecoration: "underline red" },
			children: ["a", jsx("span", { style: { textDecoration: "line-through" }, children: "b" })],
		}),
	);
	assert.deepEqual(calls, [
		[
			"paragraph",
			[
				run(0, "a", { underline: true, decorationColor: [255, 0, 0] }),
				run(0, "b", { lineThrough: true }),
			],
			"left",
		],
	]);
});
