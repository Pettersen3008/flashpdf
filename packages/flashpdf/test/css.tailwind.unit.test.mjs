import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { jsx } from "react/jsx-runtime";
import { test } from "vitest";

import { resolveStyles } from "../dist/css.js";
import { stylesheet } from "../dist/index.js";
import { boxStyle, color } from "../dist/style.js";
import { resolveTree } from "../dist/tree.js";

const resolve = async (value, sheets, width) =>
	resolveStyles(await resolveTree(value), sheets, width);
const one = async (css, className = "a", width) => {
	const [main] = await resolve(jsx("main", { className, children: "x" }), [css], width);
	return main.style;
};
const rejects = (css, message, className = "a", width) =>
	assert.rejects(one(css, className, width), message);

test("given calc(), min(), max(), and clamp() over lengths and numbers, when resolving styles, then evaluates them with CSS precedence after var() substitution", async () => {
	const s = await one(`.a {
		--s: 0.25rem;
		padding-top: calc(var(--s) * 4 + 2pt * (1 + 2));
		padding-right: max(4pt, calc(1rem - 3px));
		padding-bottom: clamp(2pt, 1px * 10, 6pt);
		padding-left: min(10px, 1rem);
		margin: calc(2px + 1pt) 0;
		line-height: calc(1.75 / 1.125);
	}`);
	assert.deepEqual(boxStyle(s, 12).padding, [18, 9.75, 6, 7.5]);
	assert.deepEqual(boxStyle(s, 12).margin, [2.5, 0, 2.5, 0]);
	assert.equal(s.lineHeight, "1.555556");
});

test("given em inside calc(), when resolving styles, then font-size resolves against the parent and other properties against the element", async () => {
	const [main] = await resolve(
		jsx("main", {
			style: { fontSize: "20pt" },
			children: jsx("p", { className: "a", children: "x" }),
		}),
		[".a { font-size: calc(1em + 2pt); padding-top: calc(1em + 2pt) }"],
	);
	const [paragraph] = main.children;
	assert.equal(paragraph.style.fontSize, 22);
	assert.equal(boxStyle(paragraph.style, 22).padding[0], 24);
});

test("given a percentage in calc(), when resolving styles, then allows a whole percentage where the property takes one and rejects the rest with a reason", async () => {
	assert.equal((await one(".a { width: calc(1 / 2 * 100%) }")).width, "50%");
	assert.equal((await one(".a { width: max(25%, 50%) }")).width, "50%");
	await rejects(
		".a { width: calc(100% - 4pt) }",
		/a percentage cannot combine with a length.*on <main\.a>/,
	);
	await rejects(".a { width: min(100%, 200pt) }", /a percentage cannot combine with a length/);
	await rejects(".a { padding: calc(50%) }", /padding does not take a percentage/);
});

test("given invalid calc() input, when resolving styles, then rejects it naming the expression and the reason", async () => {
	await rejects(
		".a { padding: calc(1 + 2pt) }",
		/invalid calc\(1 \+ 2pt\) in padding: cannot add a number and a length/,
	);
	await rejects(".a { padding: calc(2pt * 2pt) }", /cannot multiply two lengths/);
	await rejects(".a { padding: calc(4pt / 2pt) }", /the divisor must be a number/);
	await rejects(".a { padding: calc(4pt / 0) }", /division by zero/);
	await rejects(".a { padding: calc(1pt +2pt) }", /unexpected "\+2pt"/);
	await rejects(".a { padding: calc(3vh) }", /invalid length: 3vh/);
	await rejects(".a { padding: clamp(1pt, 2pt) }", /clamp\(\) takes 3 values/);
	await rejects(`.a { padding: calc(${"(".repeat(40)}1${")".repeat(40)}) }`, /nested too deeply/);
	await rejects(`.a { padding: calc(${"1pt + ".repeat(300)}1pt) }`, /longer than 1024 characters/);
});

test("given logical margin and padding properties, when resolving styles, then maps them to left-to-right, top-to-bottom sides in cascade order", async () => {
	const s = await one(`.a {
		padding-inline: 4pt 8pt; padding-block: 2pt;
		margin-inline-start: 6pt; margin-block-end: 10pt; margin-inline-end: 1pt;
		padding-left: 9pt;
	}`);
	assert.deepEqual(boxStyle(s, 12).padding, [2, 8, 2, 9]);
	assert.deepEqual(boxStyle(s, 12).margin, [0, 1, 10, 6]);
	await rejects(".a { padding-inline: 1pt 2pt 3pt }", /invalid paddingInline: 1pt 2pt 3pt/);
	const css = ".a { margin-block: 1pt; padding-inline-end: 2pt }";
	assert.equal(stylesheet(css), css);
});

test("given border-style, when resolving styles, then none and hidden clear the width in any order, solid draws it, and other styles reject", async () => {
	const widths = async (css) => boxStyle(await one(css), 12).border;
	assert.deepEqual(
		await widths(".a { border: 1pt solid red; border-bottom-style: none }"),
		[1, 1, 0, 1],
	);
	assert.deepEqual(await widths(".a { border-style: none; border-width: 2pt }"), [0, 0, 0, 0]);
	assert.deepEqual(await widths(".a { border-width: 2pt; border-style: hidden }"), [0, 0, 0, 0]);
	assert.deepEqual(
		await widths(".a { border-style: solid; border-left-width: 3pt }"),
		[0, 0, 0, 3],
	);
	await rejects(
		".a { border-style: dashed }",
		/unsupported border style: dashed \(only solid, none, and hidden are drawn\)/,
	);
	await rejects(".a { border-top-style: dotted }", /unsupported border style: dotted/);
});

test("given an @property rule, when an element sets the property, then initial-value is the default, inherits: false stops inheritance, and it beats a var() fallback", async () => {
	const css = `@property --edge { syntax: "*"; inherits: false; initial-value: solid; }
		@property --tone { syntax: "*"; inherits: true; initial-value: green; }
		@property --bare { syntax: "*"; inherits: false; }
		.p { --edge: none; --tone: red; --bare: black }
		.c { border-style: var(--edge); border-width: 1pt; color: var(--tone, blue); background-color: var(--bare, white) }`;
	const [parent] = await resolve(
		jsx("main", { className: "p", children: jsx("p", { className: "c", children: "x" }) }),
		[css],
	);
	const [child] = parent.children;
	assert.deepEqual(boxStyle(child.style, 12).border, [1, 1, 1, 1]);
	assert.equal(child.style.color, "red");
	assert.equal(child.style.backgroundColor, "white");
	const [alone] = await resolve(jsx("main", { className: "c", children: "x" }), [css]);
	assert.equal(alone.style.color, "green");
	assert.throws(
		() => stylesheet("@property --x { syntax: '*'; initial-value: 0 }"),
		/invalid @property --x: it needs syntax and inherits/,
	);
});

test("given escaped class selectors, when matching, then they match the unescaped class name", async () => {
	const css = String.raw`.w-1\/2 { width: 50% } .w-\[120px\] { width: 120px } .\32xl\:p-4 { padding: 4pt }
		.a\,b { color: red } .md\:\31 0 { margin: 1pt }`;
	assert.equal((await one(css, "w-1/2")).width, "50%");
	assert.equal((await one(css, "w-[120px]")).width, "120px");
	assert.deepEqual(boxStyle(await one(css, "2xl:p-4"), 12).padding, [4, 4, 4, 4]);
	assert.equal((await one(css, "a,b")).color, "red");
	assert.deepEqual(boxStyle(await one(css, "md:10"), 12).margin, [1, 1, 1, 1]);
	assert.equal((await one(css, "w-1")).width, undefined);
});

test("given @media blocks, when resolving styles, then print applies, screen is skipped, and width conditions compare the page content width", async () => {
	const css = `.a { color: red }
		@media print { .a { color: blue } }
		@media screen { .a { color: green } }
		@media (width >= 48rem) { .m { padding: 24pt } }
		@media (min-width: 30rem) { .n { margin: 1pt } }
		@media (max-width: 30rem) { .o { margin: 2pt } }
		@media (width < 48rem) { .p { margin: 3pt } }
		@media screen and (width >= 1rem) { .q { margin: 4pt } }
		@media print and (width >= 1rem) { .r { margin: 5pt } }
		@media not print, (width > 1000pt) { .s { margin: 6pt } }
		@media print { @media (width >= 48rem) { .t { margin: 7pt } } }`;
	assert.equal((await one(css)).color, "blue");
	const widths = async (className, width) => boxStyle(await one(css, className, width), 12);
	// 48rem is 576 pt: an A4 page with 36 pt margins is 523 pt wide, and with no margin 595 pt.
	assert.equal((await widths("m", 523))?.padding[0], undefined);
	assert.equal((await widths("m", 595))?.padding[0], 24);
	assert.equal((await widths("n", 523))?.margin[0], 1);
	assert.equal((await widths("o", 523))?.margin[0], undefined);
	assert.equal((await widths("p", 523))?.margin[0], 3);
	assert.equal((await widths("p", 595))?.margin[0], undefined);
	assert.equal((await widths("q", 523))?.margin[0], undefined);
	assert.equal((await widths("r", 523))?.margin[0], 5);
	assert.equal((await widths("s", 523))?.margin[0], undefined);
	assert.equal((await widths("t", 523))?.margin[0], undefined);
	assert.equal((await widths("t", 595))?.margin[0], 7);
});

test("given an @media query outside the subset, when validating and resolving, then stylesheet() accepts it and render rejects only where a rule matches", async () => {
	const css = `@media (prefers-color-scheme: dark) { .d { color: white } }
		@media (width >= 48rem) { .e { letter-spacing: 1pt } }`;
	assert.throws(() => stylesheet(css), /letter-spacing/);
	const dark = "@media (prefers-color-scheme: dark) { .d { color: white } }";
	assert.equal(stylesheet(dark), dark);
	assert.equal((await one(dark, "other")).color, undefined);
	await rejects(
		dark,
		/unsupported media query "\(prefers-color-scheme: dark\)" \(only all, print, screen, and width conditions are supported\) in selector "\.d" at 1:39 on <main\.d>/,
		"d",
	);
	assert.throws(
		() => stylesheet("@media print {".repeat(17) + ".a { color: red }" + "}".repeat(17)),
		/@media nests deeper than 16 levels/,
	);
});

const fixture = readFileSync(new URL("./fixtures/tailwind-invoice.css", import.meta.url), "utf8");

test("given a Tailwind v4 build of invoice utilities, when resolving styles, then spacing, borders, type, and width utilities apply", async () => {
	stylesheet(fixture);
	const style = async (className, width) => one(fixture, className, width);
	const card = await style("p-4 border border-gray-200 bg-gray-100 text-sm text-right");
	const box = boxStyle(card, 12);
	assert.deepEqual(box.padding, [12, 12, 12, 12]);
	assert.deepEqual(box.border, [0.75, 0.75, 0.75, 0.75]);
	assert.deepEqual(box.borderColor[0], color("oklch(92.8% 0.006 264.531)"));
	assert.deepEqual(box.background, color("oklch(96.7% 0.003 264.542)"));
	assert.equal(card.fontSize, 10.5);
	assert.equal(card.lineHeight, "1.428571");
	assert.equal(card.textAlign, "right");
	assert.deepEqual(boxStyle(await style("px-4 py-2 mt-4 m-2"), 12).padding, [6, 12, 6, 12]);
	assert.deepEqual(boxStyle(await style("border-b"), 12).border, [0, 0, 0.75, 0]);
	const widths = [await style("w-1/2"), await style("w-32"), await style("w-[120px]")];
	assert.deepEqual(
		widths.map((s) => s.width),
		["50%", "96pt", "120px"],
	);
	const column = await style("flex flex-col gap-4");
	assert.deepEqual([column.display, column.flexDirection, column.gap], ["flex", "column", "12pt"]);
	assert.equal(boxStyle(await style("p-4 md:p-8", 523), 12).padding[0], 12);
	assert.equal(boxStyle(await style("p-4 md:p-8", 595), 12).padding[0], 24);
	const [leading] = await resolve(
		jsx("main", {
			className: "leading-6",
			children: jsx("p", { className: "text-sm", children: "x" }),
		}),
		[fixture],
	);
	assert.equal(leading.style.lineHeight, "18pt");
	assert.equal(leading.children[0].style.lineHeight, "1.428571", "--tw-leading does not inherit");
});
