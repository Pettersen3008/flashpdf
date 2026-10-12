import assert from "node:assert/strict";

import { test } from "vitest";

import { jsx, jsxs } from "../dist/jsx-runtime.js";
import { resolveTree } from "../dist/tree.js";

const resolve = async (children, props = { viewBox: "0 0 100 100" }) => {
	const [svg] = await resolveTree(jsx("svg", { ...props, children }));
	return svg;
};
const round = (values) => values.map((value) => Math.round(value * 1000) / 1000);

test("given long unterminated XML tokens, when reading SVG bytes, then rejects at the first token", async () => {
	for (const markup of [
		"<!--".repeat(100_000),
		"<?".repeat(100_000),
		"<!DOCTYPE".repeat(10_000),
		`<svg ${"-".repeat(100_000)}/>`,
		"<!-- malformed --!><svg/>",
		"<!-- malformed --!> still malformed --><svg/>",
	])
		await assert.rejects(
			resolveTree(jsx("img", { src: new TextEncoder().encode(markup) })),
			/unsupported SVG markup at offset 0/,
		);
});

test("given path data with relative, smooth, quadratic, and arc commands, when resolving, then emits absolute moves, lines, and cubics", async () => {
	const svg = await resolve([
		jsx("path", { d: "m10 10 h5 v5 l-5 0 z l1 1" }),
		jsx("path", { d: "M0 0 Q 10 10 20 0 T 40 0" }),
		jsx("path", { d: "M0 0 A10 10 0 0 1 20 0" }),
	]);
	const [lines, quadratic, arc] = svg.drawing.shapes;
	assert.deepEqual(lines.ops, [0, 1, 1, 1, 3, 0, 1]);
	assert.deepEqual(lines.points, [10, 10, 15, 10, 15, 15, 10, 15, 10, 10, 11, 11]);
	assert.deepEqual(quadratic.ops, [0, 2, 2]);
	assert.deepEqual(
		round(quadratic.points),
		[0, 0, 6.667, 6.667, 13.333, 6.667, 20, 0, 26.667, -6.667, 33.333, -6.667, 40, 0],
	);
	assert.deepEqual(arc.ops, [0, 2, 2]);
	// The sweep flag turns through negative y, so the half circle peaks at (10, -10).
	assert.deepEqual(round(arc.points.slice(6, 8)), [10, -10]);
	assert.deepEqual(arc.points.slice(-2), [20, 0]);
});

test("given a group with a transform and paint, when resolving, then its shapes inherit both and style beats attributes", async () => {
	const svg = await resolve(
		jsxs("g", {
			transform: "translate(5 5) scale(2)",
			fill: "red",
			opacity: 0.5,
			children: [
				jsx("rect", { width: 10, height: 10, rx: 2, fillOpacity: "50%" }),
				jsx("circle", {
					r: 4,
					fill: "blue",
					style: "fill: currentColor; stroke-width: 3",
					stroke: "#000",
				}),
				jsx("line", { x2: 10, fill: "red" }),
			],
		}),
		{ width: 40, viewBox: "0 0 20 10", fill: "green" },
	);
	const [rect, circle] = svg.drawing.shapes;
	assert.equal(svg.drawing.shapes.length, 2, "a line has no fill and no stroke by default");
	assert.deepEqual(rect.matrix, [2, 0, 0, 2, 5, 5]);
	assert.deepEqual([rect.fill, rect.fillOpacity, rect.opacity], [[255, 0, 0], 0.5, 0.5]);
	assert.equal(rect.ops.filter((op) => op === 2).length, 4, "four rounded corners");
	assert.deepEqual(
		[circle.fill, circle.stroke, circle.strokeWidth],
		["currentColor", [0, 0, 0], 3],
	);
	assert.deepEqual([svg.drawing.width, svg.drawing.height], [30, 15]);
});

test("given unsupported SVG input, when resolving, then rejects with the reason and the element", async () => {
	for (const [children, message] of [
		[jsx("text", { children: "Hi" }), /SVG text is not supported/],
		["Hi", /SVG text is not supported/],
		[jsx("linearGradient", {}), /gradients and patterns are not supported/],
		[jsx("rect", { width: 1, height: 1, fill: "url(#g)" }), /fill: gradients and patterns/],
		[
			jsx("rect", { fill: "rgba(0, 0, 0, 0.5)" }),
			/fill: a colour with alpha is not supported; use fillOpacity/,
		],
		[jsx("pattern", {}), /gradients and patterns are not supported/],
		[jsx("filter", {}), /filters are not supported/],
		[jsx("mask", {}), /masks are not supported/],
		[jsx("clipPath", {}), /clipPath is not supported/],
		[jsx("use", {}), /use, symbol, and defs are not supported/],
		[jsx("image", {}), /<image> inside <svg> is not supported/],
		[jsx("foreignObject", {}), /foreignObject is not supported/],
		[jsx("style", { children: "rect { fill: red }" }), /CSS inside <svg> is not supported/],
		[jsx("rect", { className: "a" }), /CSS classes do not apply inside <svg>/],
		[jsx("rect", { filter: "url(#f)" }), /unsupported attribute filter on <rect>/],
		[jsx("rect", { style: { color: "red" } }), /unsupported style property inside <svg>: color/],
		[
			jsx("rect", { style: "constructor: x" }),
			/unsupported style property inside <svg>: constructor/,
		],
		[jsx("constructor", {}), /unsupported SVG element <constructor>/],
		[jsx("rect", { transform: "perspective(2)" }), /invalid transform/],
		[jsx("path", { d: "L 1 1" }), /invalid path data at offset 0: path data must start with M/],
		[jsx("path", { d: "M0 0 A 1 1 0 2 0 1 1" }), /expected an arc flag, 0 or 1/],
		[jsx("path", { d: `M0 0${" h1".repeat(100_000)}` }), /more than 100000 path segments/],
		[jsx("polygon", { points: "0 0 1" }), /even number of coordinates/],
		[
			jsx("g", { children: jsx("g", { children: jsx("circle", { r: -1 }) }) }),
			/invalid r: -1 on <circle> in <g> > <g> > <svg> > <main>$/,
		],
	])
		await assert.rejects(
			resolveTree(jsx("main", { children: jsx("svg", { viewBox: "0 0 1 1", children }) })),
			message,
		);
	await assert.rejects(
		resolve([], {}),
		/an <svg> needs a viewBox, or both width and height on <svg>/,
	);
	await assert.rejects(
		resolve([], { viewBox: "0 0 1 1", preserveAspectRatio: "xMidYMid defer" }),
		/invalid preserveAspectRatio/,
	);
});

test("given SVG file bytes in an img, when resolving, then builds the drawing the same JSX builds", async () => {
	const file = `<?xml version="1.0" encoding="UTF-8"?>
<!-- exported -->
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="48" fill="none">
	<title>Check &amp; mark</title>
	<path d="M5 12l5 5L20 7" stroke="#000" stroke-width="2" style="stroke-linecap: round"/>
</svg>`;
	const [image] = await resolveTree(
		jsx("img", { src: new TextEncoder().encode(file), style: { width: 30 } }),
	);
	const svg = await resolve(
		[
			jsx("title", { children: "Check & mark" }),
			jsx("path", { d: "M5 12l5 5L20 7", stroke: "#000", strokeWidth: 2, strokeLinecap: "round" }),
		],
		{ viewBox: "0 0 24 24", width: 48, fill: "none" },
	);
	assert.deepEqual(image.drawing, svg.drawing);
	assert.equal(image.alt, "Check & mark");
	for (const [markup, message] of [
		['<!DOCTYPE svg [<!ENTITY x "y">]><svg/>', /unsupported SVG markup at offset 0/],
		[
			'<svg viewBox="0 0 1 1"><rect fill="&nbsp;"/></svg>',
			/unsupported entity in SVG markup: &nbsp;/,
		],
		['<svg viewBox="0 0 1 1"><g></svg>', /SVG markup closes <svg> where <g> is open/],
		['<svg class="logo" viewBox="0 0 1 1"/>', /style and class on the SVG root are not supported/],
		[
			'<svg viewBox="0 0 1 1"><sodipodi:namedview/></svg>',
			/unsupported SVG element <sodipodi:namedview>/,
		],
	])
		await assert.rejects(
			resolveTree(jsx("main", { children: jsx("img", { src: new TextEncoder().encode(markup) }) })),
			message,
		);
});
