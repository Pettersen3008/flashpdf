import assert from "node:assert/strict";

import { jsx, jsxs } from "react/jsx-runtime";
import { test } from "vitest";

import { compile } from "../dist/compile.js";
import { resolveStyles } from "../dist/css.js";
import { ProtocolWriter } from "../dist/protocol.js";
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

/** Records written through the real writer, which collapses margins: a spacer with its height,
 *  a box start with its top and bottom margins. */
async function written(element) {
	const records = [];
	let values = [];
	const binary = new Proxy(
		{},
		{
			get: (_, method) =>
				method === "record"
					? (opcode, payload) => {
							values = [];
							payload?.();
							records.push(
								opcode === 2
									? ["spacer", values[0]]
									: opcode === 17
										? ["boxStart", values[0], values[2]]
										: { 7: "pageBreak", 9: "paragraph", 18: "boxEnd" }[opcode],
							);
						}
					: (value) => values.push(value),
		},
	);
	const fonts = new Map([["Helvetica", helvetica]]);
	compile(resolveStyles(await resolveTree(element)), new ProtocolWriter(binary, {}), fonts);
	return records;
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
	kerning: true,
	rise: 0,
	background: undefined,
	marker: 0,
	whiteSpace: "normal",
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
		/<p> accepts only text, <br>, and inline elements such as <span>, <b>, <a>, and <code> without box styles on <p>/,
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
			["stackStart", 0, false],
			["paragraph", [run(font, text)], "left"],
			["stackEnd", false],
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
	const cell = (text) => [
		["stackStart", 0, false],
		["paragraph", [run(0, text)], "left"],
		["stackEnd", false],
	];
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
		["flush"],
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
		["flush"],
		["spacer", 10],
		["flush"],
		["paragraph", [run(0, "c "), run(1, "d")], "left"],
		["flush"],
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

/** The runs of every paragraph a compiled document writes, in order. */
const paragraphs = (calls) =>
	calls.filter(([method]) => method === "paragraph").map(([, runs]) => runs);

test("given a ul and an ol with start, reversed, and an li value, when compiling, then each item's first run is a marker that numbers from them", async () => {
	const item = (children, props = {}) => jsx("li", { children, ...props });
	const calls = await compiled(
		jsxs("main", {
			children: [
				jsxs("ul", { children: [item("a"), item("b")] }),
				jsxs("ol", { start: 3, children: [item("c"), item("d", { value: 10 }), item("e")] }),
				jsxs("ol", { reversed: true, children: [item("f"), item("g")] }),
			],
		}),
	);
	const markers = paragraphs(calls).map(([marker, text]) => [
		marker.marker,
		marker.text,
		text.text,
	]);
	assert.deepEqual(markers, [
		[1, "\u2022", "a"],
		[1, "\u2022", "b"],
		[1, "3. ", "c"],
		[1, "10. ", "d"],
		[1, "11. ", "e"],
		[1, "2. ", "f"],
		[1, "1. ", "g"],
	]);
});

test("given lists nested three deep, when compiling, then ul markers cycle disc, circle, square and ol stays decimal", async () => {
	const list = (tag, children) => jsx(tag, { children });
	const item = (...children) => jsx("li", { children });
	const calls = await compiled(
		list("ul", [item("a", list("ul", [item("b", list("ul", [item("c")]))]))]),
	);
	const calls2 = await compiled(list("ol", [item("x", list("ul", [item("y")]))]));
	assert.deepEqual(
		paragraphs(calls).map(([marker]) => marker.text),
		["\u2022", "\u25e6", "\u25aa"],
	);
	assert.deepEqual(
		paragraphs(calls2).map(([marker]) => marker.text),
		["1. ", "\u25e6"],
	);
});

test("given list-style-position inside and type none, when compiling, then the marker is inside or absent while the count goes on", async () => {
	const item = (children) => jsx("li", { children });
	const calls = await compiled(
		jsxs("ol", {
			style: { listStyle: "upper-roman inside" },
			children: [
				item("a"),
				jsx("li", { style: { listStyleType: "none" }, children: "b" }),
				item("c"),
			],
		}),
	);
	const runs = paragraphs(calls);
	assert.deepEqual(
		runs.map((paragraph) => [paragraph[0].marker, paragraph[0].text]),
		[
			[2, "I. "],
			[0, "b"],
			[2, "III. "],
		],
	);
});

test("given an li outside a list, a ul with another child, or a value in a ul, when compiling, then rejects", async () => {
	await assert.rejects(compiled(jsx("li", { children: "x" })), /<li> belongs in a <ul> or <ol>/);
	await assert.rejects(
		compiled(jsx("ul", { children: jsx("p", { children: "x" }) })),
		/<ul> accepts only <li> on <ul>/,
	);
	await assert.rejects(
		compiled(jsx("ul", { children: jsx("li", { value: 2, children: "x" }) })),
		/value applies only to an <li> in an <ol>/,
	);
});

test("given an item that starts with a nested list, when compiling, then its marker takes a line of its own", async () => {
	const calls = await compiled(
		jsx("ul", {
			children: jsx("li", { children: jsx("ul", { children: jsx("li", { children: "in" }) }) }),
		}),
	);
	assert.deepEqual(
		paragraphs(calls).map(([first, second]) => [first.marker, first.text, second?.text]),
		[
			[1, "\u2022", "\u00a0"],
			[1, "\u25e6", "in"],
		],
	);
});

test("given pre text with tabs, runs of spaces, and newlines, when compiling, then each line is a run that keeps its spaces and tabs reach multiples of 8", async () => {
	const calls = await compiled(jsx("pre", { children: "\nab\tc  d\n\nend" }));
	assert.deepEqual(
		paragraphs(calls)[0].map(({ text, break: hard, font }) => [text, hard, font]),
		[
			["ab\u00a0\u00a0\u00a0\u00a0\u00a0\u00a0c\u00a0\u00a0d", true, 8],
			["", true, 8],
			["end", false, 8],
		],
	);
});

test("given pre-wrap, pre-line, and nowrap, when compiling, then spaces keep one wrap point, collapse, or never break", async () => {
	const lines = async (whiteSpace, text) =>
		paragraphs(await compiled(jsx("p", { style: { whiteSpace }, children: text })))[0].map(
			({ text }) => text,
		);
	assert.deepEqual(await lines("pre-wrap", "  a   b\nc"), ["\u00a0\u00a0a\u00a0\u00a0 b", "c"]);
	assert.deepEqual(await lines("pre-line", "a   b\n c"), ["a   b", " c"]);
	assert.deepEqual(await lines("nowrap", "  a \n  b  "), ["a\u00a0b"]);
});

test("given sub and sup inside each other and a mark, when compiling, then the rise accumulates, the size shrinks, and the mark carries its background", async () => {
	const calls = await compiled(
		jsxs("p", {
			children: [
				"x",
				jsx("sup", { children: ["2", jsx("sub", { children: "i" })] }),
				jsx("mark", { children: "m" }),
			],
		}),
	);
	const [[x, two, i, mark]] = paragraphs(calls);
	// A sup lifts a third of the parent's size and a nested sub lowers a fifth of the sup's.
	assert.deepEqual([x.rise, two.rise], [0, 0.33 * 12]);
	assert.ok(Math.abs(i.rise - (0.33 * 12 - 0.2 * two.size)) < 1e-9);
	assert.ok(Math.abs(two.size - 12 * 0.83) < 1e-5 && i.size < two.size);
	assert.deepEqual(
		[mark.background, mark.color],
		[
			[255, 255, 0],
			[0, 0, 0],
		],
	);
});

test("given vertical-align on a block, a box, or with an unknown value, when compiling, then rejects", async () => {
	await assert.rejects(
		compiled(jsx("p", { style: { verticalAlign: "sub" }, children: "x" })),
		/vertical-align applies only to inline elements/,
	);
	await assert.rejects(
		compiled(jsx("sup", { style: { padding: 2 }, children: "x" })),
		/vertical-align: super shifts text within a line, and <sup> has box styles/,
	);
	await assert.rejects(
		compiled(
			jsx("p", { children: jsx("span", { style: { verticalAlign: "middle" }, children: "x" }) }),
		),
		/invalid verticalAlign: middle/,
	);
});

test("given a p with a 10pt bottom margin before an h2 with a 16pt top margin, when compiling, then one 16pt spacer separates them", async () => {
	const records = await written(
		jsxs("main", {
			children: [
				jsx("p", { style: { marginBottom: 10 }, children: "a" }),
				jsx("h2", { style: { marginTop: 16 }, children: "b" }),
			],
		}),
	);
	assert.deepEqual(records, ["paragraph", ["spacer", 16], "paragraph"]);
});

test("given a box with a left border and bottom padding, when compiling, then a child's top margin collapses through it and its bottom margin stays inside", async () => {
	const records = await written(
		jsxs("main", {
			children: [
				jsx("blockquote", {
					style: { margin: "8pt 0", borderLeft: "2pt solid", paddingBottom: 4 },
					children: jsx("p", { style: { margin: "12pt 0" }, children: "a" }),
				}),
				"b",
			],
		}),
	);
	assert.deepEqual(records, [
		["spacer", 12],
		["boxStart", 0, 0],
		"paragraph",
		["spacer", 12],
		"boxEnd",
		["spacer", 8],
		"paragraph",
	]);
});

test("given an empty bordered div with 20pt margins between paragraphs, when compiling, then its margins collapse with theirs and it writes no box", async () => {
	const records = await written(
		jsxs("main", {
			children: [
				jsx("p", { style: { marginBottom: 8 }, children: "a" }),
				jsx("div", { style: { margin: "20pt 0", borderLeft: "1pt solid" } }),
				jsx("p", { style: { marginTop: 8 }, children: "b" }),
			],
		}),
	);
	assert.deepEqual(records, ["paragraph", ["spacer", 20], "paragraph"]);
});

test("given flex column items with margins, when compiling, then their margins and their children's add up", async () => {
	const records = await written(
		jsxs("main", {
			style: { display: "flex", flexDirection: "column" },
			children: [
				jsx("div", { style: { marginBottom: 10 }, children: "a" }),
				jsx("div", { children: jsx("p", { style: { marginTop: 6 }, children: "b" }) }),
			],
		}),
	);
	assert.deepEqual(records, [
		["boxStart", 0, 10],
		"paragraph",
		"boxEnd",
		["spacer", 6],
		"paragraph",
	]);
});

test("given margins around a forced page break, when compiling, then drops the one before and keeps the one after", async () => {
	const records = await written(
		jsxs("main", {
			children: [
				jsx("p", { style: { marginBottom: 20 }, children: "a" }),
				jsx("p", { style: { breakBefore: "page", marginTop: 12 }, children: "b" }),
			],
		}),
	);
	assert.deepEqual(records, ["paragraph", "pageBreak", ["spacer", 12], "paragraph"]);
});
