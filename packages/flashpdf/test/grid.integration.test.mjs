import assert from "node:assert/strict";
import { inflateSync } from "node:zlib";

import { test } from "vitest";

import { render, stylesheet } from "../dist/index.js";
import { jsx, jsxs } from "../dist/jsx-runtime.js";

const contents = (pdf) =>
	[
		...Buffer.from(pdf)
			.toString("latin1")
			.matchAll(/stream\r?\n([\s\S]*?)\r?\nendstream/g),
	]
		.map(([, body]) => inflateSync(Buffer.from(body, "latin1")).toString("latin1"))
		.filter((stream) => stream.includes("BT\n"));
const labels = (pdf) =>
	contents(pdf).map((page) => [...page.matchAll(/\(([A-Z]+)\)/g)].map((match) => match[1]));
const page = { pageFormat: { width: 300, height: 100 }, margin: 0 };

test("given CSS and inline grids with repeat, minmax, and a span, when rendering, then match fixed tracks and place the next item on a new row", async () => {
	const cells = [
		jsx("div", { className: "wide", style: { height: 24, gridColumn: "span 2" }, children: "AA" }),
		jsx("div", { style: { height: 24 }, children: "BB" }),
		jsx("div", { style: { height: 24 }, children: "CC" }),
	];
	const inline = (columns) =>
		jsxs("main", {
			style: { display: "grid", gridTemplateColumns: columns, gap: 6, fontFamily: "monospace" },
			children: cells,
		});
	const pdf = await render(inline("repeat(3, minmax(0, 1fr))"), page);
	assert.deepEqual(pdf, await render(inline("96pt 96pt 96pt"), page));
	assert.deepEqual(
		pdf,
		await render(
			jsxs("main", {
				className: "grid",
				children: cells.map(({ type, props }) =>
					jsx(type, { className: props.className, children: props.children }),
				),
			}),
			{
				...page,
				stylesheets: [
					stylesheet(`
				.grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 6pt; font-family: monospace }
				.grid > div { height: 24pt }
				.wide { grid-column: span 2 }
			`),
				],
			},
		),
	);
	const [content] = contents(pdf);
	const positions = [...content.matchAll(/([\d.-]+) ([\d.-]+) Td\n(?:\[)?\(([A-Z]+)\)/g)].map(
		([, x, y, label]) => ({ label, x: Number(x), y: Number(y) }),
	);
	assert.deepEqual(
		positions.map(({ label, x }) => [label, x]),
		[
			["AA", 0],
			["BB", 204],
			["CC", 0],
		],
	);
	assert.equal(positions[0].y, positions[1].y);
	assert.equal(positions[0].y - positions[2].y, 30);
});

test("given independent grid rows taller together than a page, when rendering, then splits only between rows", async () => {
	const pdf = await render(
		jsxs("main", {
			style: { display: "grid", gridTemplateColumns: "repeat(2, 1fr)", gridAutoRows: 40 },
			children: ["AA", "BB", "CC", "DD", "EE", "FF"].map((label) =>
				jsx("div", { children: label }),
			),
		}),
		page,
	);
	assert.deepEqual(labels(pdf), [
		["AA", "BB", "CC", "DD"],
		["EE", "FF"],
	]);
});

test("given grid rows connected by a rowspan near a page end, when rendering, then moves the connected rows together", async () => {
	const pdf = await render(
		jsxs("main", {
			children: [
				jsx("div", { style: { height: 40 } }),
				jsxs("div", {
					style: {
						display: "grid",
						gridTemplateColumns: "repeat(2, 1fr)",
						gridTemplateRows: "repeat(3, 30pt)",
					},
					children: [
						jsx("div", { style: { gridColumn: "span 2" }, children: "AA" }),
						jsx("div", { style: { gridRow: "span 2" }, children: "BB" }),
						jsx("div", { children: "CC" }),
						jsx("div", { children: "DD" }),
					],
				}),
			],
		}),
		page,
	);
	assert.deepEqual(labels(pdf), [["AA"], ["BB", "CC", "DD"]]);
});

test("given a grid row or connected rowspan rows taller than an empty page, when rendering, then rejects instead of clipping", async () => {
	for (const row of ["120pt", "repeat(2, 60pt)"]) {
		await assert.rejects(
			render(
				jsx("main", {
					style: { display: "grid", gridTemplateRows: row },
					children: jsx("div", {
						style: { gridRow: row === "120pt" ? "auto" : "span 2" },
						children: "AA",
					}),
				}),
				page,
			),
			/PageOverflow/,
		);
	}
});

test("given unsupported grid input in inline styles or unmatched strict CSS, when rendering, then rejects with the property", async () => {
	for (const [inline, css, value] of [
		["gridAutoFlow", "grid-auto-flow", "row dense"],
		["gridTemplateColumns", "grid-template-columns", "repeat(auto-fit, 1fr)"],
		["gridTemplateColumns", "grid-template-columns", "[start] 1fr [end]"],
		["display", "display", "inline-grid"],
		["gridTemplateAreas", "grid-template-areas", '"area"'],
		["gridArea", "grid-area", "area"],
	]) {
		await assert.rejects(
			render(jsx("main", { style: { display: "grid", [inline]: value } })),
			new RegExp(`(?:${inline}|${css})`),
		);
		await assert.rejects(
			render(jsx("main", { children: "AA" }), {
				stylesheets: [stylesheet(`.unused { ${css}: ${value} }`)],
			}),
			new RegExp(`(?:${inline}|${css}).*\\.unused`),
		);
	}
});
