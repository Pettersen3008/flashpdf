import assert from "node:assert/strict";

import { Fragment, jsx } from "react/jsx-runtime";
import { test } from "vitest";

import { resolveStyles } from "../dist/css.js";
import { stylesheet } from "../dist/index.js";
import { boxStyle, color, style } from "../dist/style.js";
import { resolveTree } from "../dist/tree.js";

const resolve = async (value, sheets) => resolveStyles(await resolveTree(value), sheets);

test("given CSS outside the supported subset, when parsing a stylesheet, then rejects it with its source position", () => {
	for (const [source, message] of [
		[
			".a { line-height: 1.5 }",
			/line-height \(line height is fixed to the font's ascent, descent, and line gap\).*"\.a" at 1:1/,
		],
		[
			".a { grid-template-columns: 1fr }",
			/unsupported CSS property: grid-template-columns.*"\.a" at 1:1/,
		],
		[".a { color }", /invalid CSS declaration: color.*at 1:1/],
		[".a { color: #000 } oops", /invalid CSS stylesheet at 1:20/],
		["{bad}p { color: red }", /invalid CSS stylesheet/],
		["@layer x { .a { color: red }", /invalid CSS stylesheet at 1:1/],
	]) {
		assert.throws(() => stylesheet(source), message, source);
	}
});

test("given at-rules and selectors outside the subset, when validating a stylesheet, then skips them instead of failing", () => {
	const source = `@import "x.css"; @layer a, b; @media print { .a { line-height: 1 } }
		.a:hover, a[href], .a > *, :root, .md\\:flex, .a + .b, . { line-height: 1; content: "}" }
		.a { color: red }`;
	assert.equal(stylesheet(source), source);
});

test("given a string and a tagged template, when calling stylesheet, then returns the joined source", () => {
	assert.equal(stylesheet(".a { color: red }"), ".a { color: red }");
	assert.equal(stylesheet`.a { color: ${"red"} }`, ".a { color: red }");
});

test("given a descendant step after a child step, when matching, then backtracks over intermediate ancestors", async () => {
	const [div] = await resolve(
		jsx("div", {
			children: jsx("section", {
				children: jsx("section", { children: jsx("p", { children: "x" }) }),
			}),
		}),
		["div > section p { color: red }"],
	);
	assert.equal(div.children[0].children[0].children[0].style.color, "red");
});

test("given one id against eleven classes, when cascading, then the id wins", async () => {
	const classes = Array.from({ length: 11 }, (_, index) => `c${index}`);
	const [main] = await resolve(
		jsx("main", { id: "x", className: classes.join(" "), children: "x" }),
		[`#x { color: red } ${classes.map((name) => `.${name}`).join("")} { color: blue }`],
	);
	assert.equal(main.style.color, "red");
});

test("given !important declarations, when cascading, then they beat higher specificity and inline styles", async () => {
	const [main] = await resolve(
		jsx("main", { id: "x", className: "a", style: { color: "green" }, children: "x" }),
		["#x { color: blue } .a { color: red !important }"],
	);
	assert.equal(main.style.color, "red");
	const [inlineWins] = await resolve(
		jsx("main", { className: "a", style: "color: green !important", children: "x" }),
		[".a { color: red !important }"],
	);
	assert.equal(inlineWins.style.color, "green");
});

const tailwind = `/*! tailwindcss v4.1.0 | MIT License | https://tailwindcss.com */
@layer properties {
  @supports ((-webkit-hyphens: none) and (not (margin-trim: inline))) or ((-moz-orient: inline) and (not (color:rgb(from red r g b)))) {
    *, :before, :after, ::backdrop { --tw-font-weight: initial; }
  }
}
@layer theme, base, components, utilities;
:root, :host { --spacing: .25rem; --color-red-500: oklch(63.7% 0.237 25.331); }
*, ::before, ::after { box-sizing: border-box; border: 0 solid; }
.p-4 { padding: 1rem; }
.p-5 { padding: calc(var(--spacing) * 5); }
.text-red-500 { color: oklch(63.7% 0.237 25.331); }
.font-bold { --tw-font-weight: var(--font-weight-bold); font-weight: var(--tw-font-weight); }
.md\\:flex { display: flex; }
@property --tw-font-weight { syntax: "*"; inherits: false; initial-value: 0; }
@media (width >= 48rem) { .md\\:flex { display: flex; } }
`;

test("given a Tailwind v4 sheet, when only .p-4 is used, then unused rules are ignored and the padding applies", async () => {
	stylesheet(tailwind);
	const [main] = await resolve(jsx("main", { className: "p-4", children: "x" }), [tailwind]);
	assert.deepEqual(boxStyle(main.style, 12).padding, [12, 12, 12, 12]);
});

test("given a Tailwind v4 sheet, when a rule with calc(var(--tw-*)) is used, then names the variable and element", async () => {
	const used = await resolveTree(jsx("main", { className: "p-5", children: "x" }));
	assert.throws(
		() => resolveStyles(used, [tailwind]),
		/undefined CSS variable: --spacing .* on <main\.p-5>/,
	);
});

test("given repeated comment markers, when parsing a stylesheet, then rejects without backtracking", () => {
	assert.throws(() => stylesheet(`/*${"a/*".repeat(10_000)}`), /invalid CSS stylesheet/);
});

test("given bounded CSS variables, when resolving styles, then rejects cycles and oversized expansion", async () => {
	const element = jsx("main", {
		style: { "--x": "var(--x)var(--x)", color: "var(--x)" },
		children: "x",
	});
	const cyclic = await resolveTree(element);
	assert.throws(() => resolveStyles(cyclic), /cyclic CSS variable/);
	const oversized = await resolveTree(
		jsx("main", {
			style: { "--x": "x".repeat(64 * 1024 + 1), color: "var(--x)" },
			children: "x",
		}),
	);
	assert.throws(() => resolveStyles(oversized), /CSS variable output is too large/);
});

test("given fragments, inherited lengths, and stylesheet numbers, when resolving styles, then computes each child once", async () => {
	const [heading] = await resolve(
		jsx(Fragment, {
			children: jsx("h1", { className: "title", children: "Title" }),
		}),
		[".title { color: red }"],
	);
	assert.equal(heading.style.fontSize, 24);
	assert.equal(heading.style.fontWeight, "bold");
	assert.equal(heading.style.color, "red");
	const [document] = await resolve(
		jsx("main", {
			style: { fontSize: "2em" },
			children: jsx("div", { children: jsx("p", { children: "x" }) }),
		}),
	);
	assert.equal(document.style.fontSize, 24);
	assert.equal(document.children[0].style.fontSize, 24);
	assert.equal(document.children[0].children[0].style.fontSize, 24);
	const [flex] = await resolve(jsx("main", { className: "cell", children: "x" }), [
		".cell { flex: 1 }",
	]);
	assert.equal(flex.style.flex, 1);
});

test("given cascading shorthands, selector whitespace, and transparent paint, when normalizing styles, then keeps CSS semantics", async () => {
	const invalid = await resolveTree(
		jsx("main", { children: jsx("p", { className: "x", children: "x" }) }),
	);
	assert.throws(
		() =>
			resolveStyles(invalid, [
				"main\n\tp.x { color: red; background: transparent; border: 1pt dashed red }",
			]),
		/unsupported border style: dashed/,
	);
	const [resolved] = await resolve(
		jsx("main", { children: jsx("p", { className: "x", children: "x" }) }),
		["main\n\tp.x { color: red; background: transparent }"],
	);
	assert.equal(resolved.children[0].style.color, "red");
	const [cascade] = await resolve(jsx("main", { style: { margin: 0 }, children: "x" }), [
		"main { margin-top: 12pt }",
	]);
	assert.deepEqual(boxStyle(cascade.style, 12).margin, [0, 0, 0, 0]);
	const [reordered] = await resolve(jsx("main", { className: "x", children: "x" }), [
		".x { margin-top: 1pt } .x { margin: 2pt } .x { margin-top: 3pt }",
	]);
	assert.deepEqual(boxStyle(reordered.style, 12).margin, [3, 2, 2, 2]);
	assert.equal(boxStyle(style({ background: "transparent" }), 12).background, undefined);
	// A colour the border shorthand does not hardcode still reaches `color`.
	assert.deepEqual(boxStyle(style({ border: "1pt solid #336699" }), 12).borderColor, [
		[0x33, 0x66, 0x99],
		[0x33, 0x66, 0x99],
		[0x33, 0x66, 0x99],
		[0x33, 0x66, 0x99],
	]);
	assert.deepEqual(boxStyle(style({ border: "1pt solid transparent" }), 12).border, [0, 0, 0, 0]);
	assert.deepEqual(boxStyle(style({ border: "none" }), 12).border, [0, 0, 0, 0]);
	assert.throws(
		() => boxStyle(style({ border: "1pt solid oklch(50% 0.1 20)" }), 12),
		/unsupported color: oklch\(50% 0.1 20\)/,
	);
	assert.throws(
		() => style({ color: "transparent" }) && color("transparent"),
		/only to background/,
	);
});
