import assert from "node:assert/strict";

import { jsx, jsxs } from "react/jsx-runtime";
import { test } from "vitest";

import { compile } from "../dist/compile.js";
import { resolveStyles } from "../dist/css.js";
import { helvetica } from "../dist/style.js";
import { resolveTree } from "../dist/tree.js";

/** Records protocol calls instead of encoding them. */
async function compiled(element) {
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
	compile(resolveStyles(await resolveTree(element)), writer, new Map([["Helvetica", helvetica]]));
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
	link: undefined,
	...extra,
});

test("given text with an inline bold child, when compiling, then emits one paragraph with two runs", async () => {
	const calls = await compiled(
		jsxs("p", { children: ["Hello ", jsx("b", { children: "World" })] }),
	);
	assert.deepEqual(calls, [["paragraph", [run(0, "Hello "), run(1, "World")], "left", 2, 2]]);
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
			2,
			2,
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
		/<p> accepts only text, <br>, and <span>, <b>, <strong>, <a> without box styles on <p>/,
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
			2,
			2,
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
	const top = { align: 0, rows: 1 };
	const row = (font, ...texts) => [
		["rowStart", columns, [top, top]],
		...texts.flatMap((text) => [
			["stackStart", 0],
			["paragraph", [run(font, text)], "left", 2, 2],
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

test("given a totals row whose label spans three columns, when compiling, then each column takes its width from its first cell without colSpan", async () => {
	const cell = (text, props = {}) => jsx("td", { ...props, children: text });
	const calls = await compiled(
		jsxs("table", {
			children: [
				jsxs("tr", {
					children: [
						cell("Item", { colSpan: 2 }),
						cell("Qty", { style: { width: "40pt" } }),
						cell("Total", { style: { width: "60pt" } }),
					],
				}),
				jsxs("tr", {
					children: [
						cell("Coffee", { style: { width: "50%" } }),
						cell("Beans"),
						cell("2", { style: { width: "90pt" } }),
						cell("$8"),
					],
				}),
				jsxs("tr", { children: [cell("Subtotal", { colSpan: 3 }), cell("$8")] }),
			],
		}),
	);
	const top = { align: 0, rows: 1 };
	assert.deepEqual(
		calls.filter(([method]) => method === "rowStart"),
		[
			[top, "join", top, top],
			[top, top, top, top],
			[top, "join", "join", top],
		].map((slots) => [
			"rowStart",
			[
				{ kind: 2, value: 50 },
				{ kind: 1, value: 1 },
				{ kind: 0, value: 40 },
				{ kind: 0, value: 60 },
			],
			slots,
		]),
	);
});

test("given a cell spanning two rows beside striped rows, when compiling, then both rows sit in one stack and each row's background moves into its own cells", async () => {
	const calls = await compiled(
		jsx("table", {
			children: jsxs("tbody", {
				children: [
					jsxs("tr", {
						style: { background: "#ff0000" },
						children: [
							jsx("td", { rowSpan: 2, children: "a" }),
							jsx("td", { style: { verticalAlign: "bottom" }, children: "b" }),
						],
					}),
					jsx("tr", {
						style: { background: "#0000ff", verticalAlign: "middle" },
						children: jsx("td", { children: "c" }),
					}),
				],
			}),
		}),
	);
	const filled = (background) => [
		"boxStart",
		{
			margin: [0, 0, 0, 0],
			padding: [0, 0, 0, 0],
			border: [0, 0, 0, 0],
			borderColor: [
				[0, 0, 0],
				[0, 0, 0],
				[0, 0, 0],
				[0, 0, 0],
			],
			background,
		},
	];
	const columns = [
		{ kind: 1, value: 1 },
		{ kind: 1, value: 1 },
	];
	const text = (value) => [["stackStart", 0], ["paragraph", [run(0, value)], "left"], ["stackEnd"]];
	assert.deepEqual(calls, [
		["tableStart", 0, 0, { kind: 1, value: 1 }],
		["stackStart", 0],
		[
			"rowStart",
			columns,
			[
				{ align: 0, rows: 2 },
				{ align: 2, rows: 1 },
			],
		],
		filled([255, 0, 0]),
		...text("a"),
		["boxEnd"],
		filled([255, 0, 0]),
		...text("b"),
		["boxEnd"],
		["rowEnd"],
		["rowStart", columns, ["covered", { align: 1, rows: 1 }]],
		filled([0, 0, 255]),
		...text("c"),
		["boxEnd"],
		["rowEnd"],
		["stackEnd"],
		["tableEnd"],
	]);
});

test("given collapsed 1pt cell borders under a 2pt header rule, when compiling, then each cell draws half of every shared edge and all of each outer edge", async () => {
	const grey = "1pt solid #808080";
	const cell = (tag, text) => jsx(tag, { style: { border: grey }, children: text });
	const calls = await compiled(
		jsxs("table", {
			style: { borderCollapse: "collapse" },
			children: [
				jsx("thead", {
					children: jsxs("tr", {
						style: { borderBottom: "2pt solid black" },
						children: [cell("th", "a"), cell("th", "b")],
					}),
				}),
				jsx("tbody", { children: jsxs("tr", { children: [cell("td", "c"), cell("td", "d")] }) }),
			],
		}),
	);
	const black = [0, 0, 0];
	const gray = [128, 128, 128];
	// The row's rule merges into its cells, so the row emits no box of its own.
	assert.deepEqual(
		calls
			.filter(([method]) => method === "boxStart")
			.map(([, { border, borderColor }]) => ({ border, borderColor })),
		[
			{ border: [1, 0.5, 1, 1], borderColor: [gray, gray, black, gray] },
			{ border: [1, 1, 1, 0.5], borderColor: [gray, gray, black, gray] },
			{ border: [1, 0.5, 1, 1], borderColor: [black, gray, gray, gray] },
			{ border: [1, 1, 1, 0.5], borderColor: [black, gray, gray, gray] },
		],
	);
});

test("given table input outside the supported subset, when compiling, then rejects with the reason", async () => {
	const table = (...rows) => jsxs("table", { children: rows });
	const tr = (...cells) => jsxs("tr", { children: cells });
	const td = (props = {}) => jsx("td", { ...props, children: "x" });
	for (const [input, message] of [
		[
			table(jsx("tbody", { children: tr(td({ rowSpan: 2 })) })),
			/^Error: rowSpan 2 runs past the last row of its <tbody>; rows joined by rowSpan stay in one row group on <td> in <tr> > <tbody> > <table>$/,
		],
		[
			table(tr(td(), td({ rowSpan: 2 })), tr(td({ colSpan: 2 }))),
			/colSpan runs into a cell that rowSpan brings down from an earlier row/,
		],
		[table(tr(td({ colSpan: 2, style: { width: "50pt" } }))), /width and flex size one column/],
		[
			table(tr(td({ style: { verticalAlign: "baseline" } }))),
			/vertical-align: baseline is not implemented/,
		],
		[
			jsx("table", { style: { borderSpacing: "2pt" }, children: tr(td()) }),
			/border-spacing is not implemented/,
		],
		[
			jsx("div", { style: { verticalAlign: "middle" }, children: "x" }),
			/vertical-align applies only to <td>, <th>, <tr>, and row groups on <div>$/,
		],
		[table(tr(td({ colSpan: 0 }))), /colSpan must be an integer from 1 to 256 on <td>/],
	])
		await assert.rejects(compiled(input), message);
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

test("given orphans and widows on a block, when compiling, then its paragraphs inherit them", async () => {
	const calls = await compiled(
		jsx("div", { style: { orphans: 3, widows: "1" }, children: jsx("p", { children: "x" }) }),
	);
	assert.deepEqual(calls, [["paragraph", [run(0, "x")], "left", 3, 1]]);
});

test("given break-inside: avoid on a paragraph, when compiling, then it is one kept-together group", async () => {
	const calls = await compiled(jsx("p", { style: { breakInside: "avoid-page" }, children: "x" }));
	assert.deepEqual(
		calls.map(([method]) => method),
		["stackStart", "paragraph", "stackEnd"],
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
		["stackStart", 0],
		["paragraph", [run(0, text)], "left", 2, 2],
		["stackEnd"],
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
		[
			"rowStart",
			[
				{ kind: 1, value: 1 },
				{ kind: 1, value: 1 },
			],
		],
		["paragraph", [run(0, "a")], "left", 2, 2],
		["paragraph", [run(0, "b")], "left", 2, 2],
		["rowEnd"],
		["spacer", 10],
		["paragraph", [run(0, "c "), run(1, "d")], "left", 2, 2],
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
			2,
			2,
		],
	]);
});
