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
	color: [0, 0, 0],
	text,
	break: false,
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
		/<p> accepts only text, <br>, and <span>, <b>, <strong> without box styles on <p>/,
	);
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
		["tableStart", 1, { kind: 1, value: 1 }],
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
