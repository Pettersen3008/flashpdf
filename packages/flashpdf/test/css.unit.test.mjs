import assert from "node:assert/strict";

import { Fragment, jsx } from "react/jsx-runtime";
import { test } from "vitest";

import { resolveStyles } from "../dist/css.js";
import { stylesheet } from "../dist/index.js";
import { boxStyle, color, style } from "../dist/style.js";
import { reactTree } from "../dist/tree.js";

test("given CSS outside the supported subset, when parsing a stylesheet, then rejects it with its source position", () => {
	for (const [source, message] of [
		[
			".a { line-height: 1.5 }",
			/line-height \(line height is fixed to the font's ascent plus descent\).*"\.a" at 1:1/,
		],
		[
			".a { grid-template-columns: 1fr }",
			/unsupported CSS property: grid-template-columns.*"\.a" at 1:1/,
		],
		["@media print { .a { color: #000 } }", /unsupported CSS at-rule: @media at 1:1/],
		[".a:hover { color: #000 }", /unsupported CSS selector: \.a:hover at 1:1/],
		["a[href] { color: #000 }", /unsupported CSS selector: a\[href\] at 1:1/],
		[".a > * { color: #000 }", /unsupported CSS selector: \.a > \* at 1:1/],
		[".a { color }", /invalid CSS declaration: color.*at 1:1/],
		[".a { color: #000 } oops", /invalid CSS stylesheet at 1:19/],
		["{bad}p { color: red }", /invalid CSS stylesheet/],
		[". { color: red }", /unsupported CSS selector/],
	]) {
		assert.throws(() => stylesheet(source), message, source);
	}
});

test("given bounded CSS variables, when resolving styles, then rejects cycles and oversized expansion", () => {
	const element = jsx("main", {
		style: { "--x": "var(--x)var(--x)", color: "var(--x)" },
		children: "x",
	});
	assert.throws(() => resolveStyles(element), /cyclic CSS variable/);
	assert.throws(
		() =>
			resolveStyles(
				jsx("main", {
					style: { "--x": "x".repeat(64 * 1024 + 1), color: "var(--x)" },
					children: "x",
				}),
			),
		/CSS variable output is too large/,
	);
});

test("given fragments, inherited lengths, and stylesheet numbers, when resolving styles, then computes each child once", async () => {
	const heading = resolveStyles(
		await reactTree(
			jsx(Fragment, {
				children: jsx("h1", { className: "title", children: "Title" }),
			}),
		),
		[".title { color: red }"],
	);
	assert.equal(heading.props.style.fontSize, 24);
	assert.equal(heading.props.style.fontWeight, "bold");
	assert.equal(heading.props.style.color, "red");
	const document = resolveStyles(
		jsx("main", {
			style: { fontSize: "2em" },
			children: jsx("div", { children: jsx("p", { children: "x" }) }),
		}),
	);
	assert.equal(document.props.style.fontSize, 24);
	assert.equal(document.props.children.props.style.fontSize, 24);
	assert.equal(document.props.children.props.children.props.style.fontSize, 24);
	const flex = resolveStyles(jsx("main", { className: "cell", children: "x" }), [
		".cell { flex: 1 }",
	]);
	assert.equal(style(flex.props.style).flex, 1);
});

test("given cascading shorthands, selector whitespace, and transparent paint, when normalizing styles, then keeps CSS semantics", () => {
	const resolved = resolveStyles(
		jsx("main", { children: jsx("p", { className: "x", children: "x" }) }),
		["main\n\tp.x { color: red; background: transparent; border: 1pt dashed red }"],
	);
	assert.equal(resolved.props.children.props.style.color, "red");
	assert.throws(
		() => style(resolved.props.children.props.style),
		/unsupported border style: dashed/,
	);
	const cascade = resolveStyles(jsx("main", { style: { margin: 0 }, children: "x" }), [
		"main { margin-top: 12pt }",
	]);
	assert.deepEqual(boxStyle(style(cascade.props.style), 12).margin, [0, 0, 0, 0]);
	const reordered = resolveStyles(jsx("main", { className: "x", children: "x" }), [
		".x { margin-top: 1pt } .x { margin: 2pt } .x { margin-top: 3pt }",
	]);
	assert.deepEqual(boxStyle(style(reordered.props.style), 12).margin, [3, 2, 2, 2]);
	assert.equal(boxStyle(style({ background: "transparent" }), 12).background, undefined);
	// A colour the border shorthand does not hardcode still reaches `color`.
	assert.deepEqual(
		boxStyle(style({ border: "1pt solid #336699" }), 12).borderColor,
		[0x33, 0x66, 0x99],
	);
	assert.equal(boxStyle(style({ border: "1pt solid transparent" }), 12).border, 0);
	assert.equal(boxStyle(style({ border: "none" }), 12).border, 0);
	assert.throws(() => boxStyle(style({ border: "1pt solid gray" }), 12), /unsupported color: gray/);
	assert.throws(
		() => style({ color: "transparent" }) && color("transparent"),
		/only to background/,
	);
});
