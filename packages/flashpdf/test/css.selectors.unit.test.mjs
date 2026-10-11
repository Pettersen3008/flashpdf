import assert from "node:assert/strict";

import { Fragment, jsx } from "react/jsx-runtime";
import { test } from "vitest";

import { resolveStyles } from "../dist/css.js";
import { stylesheet } from "../dist/index.js";
import { resolveTree } from "../dist/tree.js";

const resolve = async (value, sheets) => resolveStyles(await resolveTree(value), sheets);
const item = (tag, id, children = "x") => jsx(tag, { id, children });
const walk = (nodes) =>
	nodes.flatMap((node) => (node.kind === "element" ? [node, ...walk(node.children)] : []));
/** The ids, in document order, of the elements `selector` matches in `tree`. */
const matched = async (selector, tree) =>
	walk(await resolve(tree, [`${selector} { background-color: red }`]))
		.filter((element) => element.style.backgroundColor === "red")
		.map((element) => element.id)
		.join(" ");
const list = (count, tag = "p") =>
	jsx("div", { children: Array.from({ length: count }, (_, index) => item(tag, `n${index + 1}`)) });
const rejects = (css, message) => assert.throws(() => stylesheet(css), message, css);

test("given text, fragments, components, and empty children between siblings, when matching :first-child, :last-child, and :only-child, then only resolved elements count", async () => {
	const Item = ({ id }) => item("p", id);
	const tree = jsx("main", {
		children: [
			"intro",
			jsx(Fragment, { children: [item("p", "a"), null, false] }),
			jsx(Item, { id: "b" }),
			"between",
			[item("p", "c")],
			"outro",
		],
	});
	assert.equal(await matched("p:first-child", tree), "a");
	assert.equal(await matched("p:last-child", tree), "c");
	assert.equal(await matched("p:only-child", tree), "");
	assert.equal(await matched("p:nth-child(2)", tree), "b");
	assert.equal(
		await matched("p:only-child", jsx("main", { children: ["a", item("p", "x"), "b"] })),
		"x",
	);
});

test("given An+B, odd, and even arguments, when matching :nth-child and :nth-last-child, then follows the CSS formula from either end", async () => {
	const tree = list(7);
	for (const [selector, expected] of [
		[":nth-child(odd)", "n1 n3 n5 n7"],
		[":nth-child(EVEN)", "n2 n4 n6"],
		[":nth-child(3)", "n3"],
		[":nth-child(2n+1)", "n1 n3 n5 n7"],
		[":nth-child( 3n - 1 )", "n2 n5"],
		[":nth-child(-n+3)", "n1 n2 n3"],
		[":nth-child(n+5)", "n5 n6 n7"],
		[":nth-child(0n+2)", "n2"],
		[":nth-child(0)", ""],
		[":nth-last-child(2)", "n6"],
		[":nth-last-child(-n+2)", "n6 n7"],
		[":nth-last-child(odd)", "n1 n3 n5 n7"],
		[":nth-last-child(2n)", "n2 n4 n6"],
	])
		assert.equal(await matched(`p${selector}`, tree), expected, selector);
});

test("given siblings of mixed tags, when matching the of-type pseudo-classes, then each counts only siblings with its own tag", async () => {
	const tree = jsx("div", {
		children: [item("h1", "a"), item("p", "b"), item("p", "c"), item("h1", "d"), item("p", "e")],
	});
	assert.equal(await matched("p:first-of-type", tree), "b");
	assert.equal(await matched("p:last-of-type", tree), "e");
	assert.equal(await matched("p:nth-of-type(2)", tree), "c");
	assert.equal(await matched("p:nth-last-of-type(odd)", tree), "b e");
	assert.equal(await matched("div > :first-of-type", tree), "a b");
	assert.equal(await matched("h1:only-of-type", tree), "");
	assert.equal(
		await matched(
			"div > :only-of-type",
			jsx("div", { children: [item("h1", "a"), item("p", "b")] }),
		),
		"a b",
	);
});

test("given elements with and without children, when matching :empty, then only one with no children or zero-length text matches", async () => {
	const tree = jsx("div", {
		children: [
			jsx("p", { id: "a" }),
			jsx("p", { id: "b", children: "" }),
			jsx("p", { id: "c", children: " " }),
			jsx("p", { id: "d", children: item("span", "e") }),
			jsx("p", { id: "f", children: [null, false] }),
		],
	});
	assert.equal(await matched("p:empty", tree), "a b f");
});

test("given one root or several, when matching :root, then each root element matches and no descendant does", async () => {
	assert.equal(await matched(":root", jsx("main", { id: "a", children: item("div", "b") })), "a");
	assert.equal(await matched(":root", [item("main", "a"), item("main", "b")]), "a b");
	const [main] = await resolve(jsx("main", { children: item("p", "b") }), [
		":root { --gap: 4pt } p { padding-top: var(--gap) }",
	]);
	assert.equal(main.children[0].style.paddingTop, "4pt");
});

test("given child, next-sibling, and subsequent-sibling combinators and *, when matching, then each follows its CSS meaning", async () => {
	const tree = jsx("div", {
		id: "d",
		children: [
			item("h1", "h1"),
			item("p", "p1"),
			item("p", "p2"),
			jsx("section", { id: "s", children: item("p", "sp") }),
			item("h1", "h2"),
			item("p", "p3"),
		],
	});
	for (const [selector, expected] of [
		["h1 + p", "p1 p3"],
		["h1 ~ p", "p1 p2 p3"],
		["h1~p", "p1 p2 p3"],
		["p + p", "p2"],
		["h1 + p + p", "p2"],
		["h1 ~ p + section", "s"],
		["h1 ~ h1", "h2"],
		["div > p", "p1 p2 p3"],
		["div p", "p1 p2 sp p3"],
		["section > *", "sp"],
		["* > p", "p1 p2 sp p3"],
		["div > * + *", "p1 p2 s h2 p3"],
		["*", "d h1 p1 p2 s sp h2 p3"],
		["* + *", "p1 p2 s h2 p3"],
		["section ~ p", "p3"],
	])
		assert.equal(await matched(selector, tree), expected, selector);
});

test("given selector lists in :is(), :where(), and :not(), when matching, then each follows Selectors 4 with complex arguments", async () => {
	const tree = jsx("div", {
		id: "d",
		className: "box",
		children: [
			jsx("h1", { id: "h", className: "a", children: "x" }),
			jsx("p", { id: "p1", className: "a b", children: "x" }),
			jsx("section", { id: "s", children: jsx("p", { id: "sp", className: "b", children: "x" }) }),
		],
	});
	for (const [selector, expected] of [
		[":is(h1, section)", "h s"],
		[":where(h1, section)", "h s"],
		[":not(h1, section)", "d p1 sp"],
		[":not(.a)", "d s sp"],
		[".a:not(.b)", "h"],
		[":is(.a, .b):not(p)", "h"],
		[":where(div > p)", "p1"],
		[":not(div > p)", "d h s sp"],
		[":is(.box p, section > p)", "p1 sp"],
		[":not(:is(h1, p))", "d s"],
		[":not(:not(h1))", "h"],
		[":is(h1:first-child, p:last-child)", "h sp"],
		[":not(:first-child)", "p1 s"],
		[":is()", ""],
		[":not(:hover)", "d h p1 s sp"],
		[":is(:hover, h1)", "h"],
		[".box :where(.b, h1) + *", "p1 s"],
	])
		assert.equal(await matched(selector, tree), expected, selector);
});

test("given :is(), :not(), :where(), and structural pseudo-classes, when cascading, then :is and :not count their most specific argument, :where counts nothing, and the rest count as classes", async () => {
	const color = async (css) => {
		const [p] = await resolve(jsx("p", { id: "x", className: "a", children: "y" }), [css]);
		return p.style.color;
	};
	assert.equal(await color(":is(#x, .a) { color: red } .a.a.a { color: blue }"), "red");
	assert.equal(await color("p:not(#nope) { color: red } .a.a.a { color: blue }"), "red");
	assert.equal(await color(".a:where(#x) { color: red } .a { color: blue }"), "blue");
	assert.equal(await color("p:first-child { color: red } .a { color: blue }"), "red");
	assert.equal(await color(":is(p, #nope) { color: red } #x { color: blue }"), "blue");
	assert.equal(await color(":not(.b, p) { color: red } :not(.b, .c, .d) { color: blue }"), "blue");
});

test("given selectors that can match no rendered element, when validating and resolving, then skips the rule even with an invalid declaration", async () => {
	const css = `.a:hover, .a:focus-visible, .a:active, a:visited, button:disabled, .a:checked { letter-spacing: 1pt }
		.a::before, .a::after, .a::placeholder, .a::selection, ::backdrop, .a:before, p::first-line { letter-spacing: 1pt }
		a[href]::after, input[type=text]:focus, .a:hover:has(.b), :is(:hover, :focus) { letter-spacing: 1pt }
		:host, :host(.a), html, body, button, .a:not(:is(div)) > svg { letter-spacing: 1pt }
		.a:-moz-focusring, .a:-webkit-autofill, .a:-ms-input-placeholder { letter-spacing: 1pt }`;
	assert.equal(stylesheet(css), css);
	await resolve(jsx("p", { className: "a", children: "x" }), [css]);
	assert.equal(await matched(":not(:hover):not(:focus)", item("p", "x")), "x");
});

test("given selectors outside the subset that could match an element, when validating a stylesheet, then rejects each with its reason and position", () => {
	for (const [css, message] of [
		["a[href] { color: red }", /attribute selectors .* in selector "a\[href\]" at 1:1/],
		[
			".a[data-x='a,b'], .c { color: red }",
			/attribute selectors .* in selector "\.a\[data-x='a,b'\]"/,
		],
		[
			".a:has(.b) { color: red }",
			/:has\(\) \(not implemented\) in selector "\.a:has\(\.b\)" at 1:1/,
		],
		[
			"\n  p:lang(en) { color: red }",
			/:lang\(\) \(not implemented\) in selector "p:lang\(en\)" at 2:3/,
		],
		[".a:dir(rtl), .b { color: red }", /:dir\(\)/],
		[":scope, :defined { color: red }", /:scope \(not implemented\)/],
		[".a:unknown { color: red }", /:unknown \(not implemented\)/],
		["*|p { color: red }", /namespaces/],
		["|p { color: red }", /namespaces/],
		[
			"p:nth-child(2 of .a) { color: red }",
			/\(use An\+B, odd, or even; "of <selector>" is not implemented\)/,
		],
		["p:nth-child(n+) { color: red }", /:nth-child\(n\+\)/],
		["p:nth-of-type() { color: red }", /:nth-of-type\(\)/],
		[":is(.a, [x]) { color: red }", /attribute selectors/],
		[":not(.a:has(.b)) { color: red }", /:has\(\)/],
		["@media print { .a[x] { color: red } }", /attribute selectors/],
		["@media (prefers-color-scheme: dark) { .a:has(.b) { color: red } }", /:has\(\)/],
	])
		rejects(css, message);
});

test("given malformed selectors, when validating a stylesheet, then rejects them instead of skipping the rule", () => {
	for (const css of [
		". { color: red }",
		".a > { color: red }",
		"> .a { color: red }",
		".a >> .b { color: red }",
		".a, { color: red }",
		"..a { color: red }",
		"#.a { color: red }",
		".a:: { color: red }",
		".a$ { color: red }",
		"*a { color: red }",
		":is(.a { color: red }",
		".a[x { color: red }",
		":not(.a,) { color: red }",
	])
		rejects(css, /invalid CSS selector in selector/);
	rejects(
		`${":is(".repeat(17)}.a${")".repeat(17)} { color: red }`,
		/:is\(\) nests deeper than 16 levels/,
	);
	assert.doesNotThrow(() => stylesheet(`${":is(".repeat(16)}.a${")".repeat(16)} { color: red }`));
});

test("given a long subsequent-sibling chain that fails on a wide list, when matching, then finishes quickly", async () => {
	// The p comes last, so every span fails the chain after trying every earlier sibling at every step; 40 siblings took 10 s uncached.
	const spans = Array.from({ length: 20_000 }, () => jsx("span", { children: "x" }));
	const tree = await resolveTree(jsx("div", { children: [...spans, jsx("p", { children: "x" })] }));
	const started = performance.now();
	const [div] = resolveStyles(tree, [`p ~ ${"span ~ ".repeat(7)}span { color: red }`]);
	assert.ok(performance.now() - started < 1000);
	assert.equal(
		div.children.some((child) => child.style.color === "red"),
		false,
	);
});

test("given :not() and :is() holding long descendant selectors on a deep tree, when matching, then finishes quickly", async () => {
	// Each argument is a full selector, so a cache-free matcher repeats the 5.6 s backtracking of #36 once per argument.
	let element = "x";
	for (let index = 0; index < 40; index++) element = jsx("div", { children: element });
	const tree = await resolveTree(element);
	const chain = `section ${"div ".repeat(8)}`;
	const started = performance.now();
	resolveStyles(tree, [
		`:not(${chain}) { color: red }`,
		`:is(${chain}, ${chain}p, ${chain}div ~ div) { color: red }`,
		`:where(${chain}) > :not(:is(${chain}, ${chain})) { color: red }`,
	]);
	assert.ok(performance.now() - started < 1000);
});
