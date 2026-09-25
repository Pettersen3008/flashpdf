import assert from "node:assert/strict";

import { test } from "vitest";

import { color, edges, font, helvetica, point, style } from "../dist/style.js";

test("given pt, px, em, and rem lengths, when resolving a point value, then converts to points", () => {
	assert.equal(point("12pt"), 12);
	assert.equal(point("16px"), 12);
	assert.equal(point("2em", 10), 20);
	assert.equal(point("2rem", 10), 24);
	assert.equal(point("0"), 0);
});

test("given an unrecognised length, when resolving a point value, then rejects it", () => {
	assert.throws(() => point("2vh"), /invalid length/);
	assert.throws(() => point("abc"), /invalid length/);
});

test("given 1, 2, 3, or 4 shorthand values, when expanding margin edges, then mirrors CSS shorthand order", () => {
	assert.deepEqual(edges({ margin: "4pt" }, "margin", 12), [4, 4, 4, 4]);
	assert.deepEqual(edges({ margin: "4pt 8pt" }, "margin", 12), [4, 8, 4, 8]);
	assert.deepEqual(edges({ margin: "4pt 8pt 12pt" }, "margin", 12), [4, 8, 12, 8]);
	assert.deepEqual(edges({ margin: "1pt 2pt 3pt 4pt" }, "margin", 12), [1, 2, 3, 4]);
});

test("given a per-side override, when expanding margin edges, then it wins over the shorthand", () => {
	assert.deepEqual(edges({ margin: "4pt", marginTop: "9pt" }, "margin", 12), [9, 4, 4, 4]);
});

test("given a named color or a 3- or 6-digit hex, when resolving a color, then converges on the same RGB triple", () => {
	assert.deepEqual(color("red"), [255, 0, 0]);
	assert.deepEqual(color("#f00"), [255, 0, 0]);
	assert.deepEqual(color("#ff0000"), [255, 0, 0]);
});

test("given rgb, hsl, named, and 4- or 8-digit hex colors, when resolving a color, then converges on the same RGB triple", () => {
	for (const value of [
		"rgb(255, 0, 0)",
		"rgb(100% 0% 0%)",
		"rgba(255 0 0 / 1)",
		"rgba(255, 0, 0, 100%)",
		"hsl(0, 100%, 50%)",
		"hsl(360deg 100% 50%)",
		"#f00f",
		"#ff0000ff",
		"Red",
	])
		assert.deepEqual(color(value), [255, 0, 0], value);
	assert.deepEqual(color("rebeccapurple"), [0x66, 0x33, 0x99]);
	assert.deepEqual(color("hsl(120 100% 25%)"), [0, 128, 0]);
});

test("given a translucent color, when resolving it, then rejects with the opacity reason", () => {
	for (const value of ["rgba(0, 0, 0, 0.5)", "#0008", "#00000080", "hsl(0 0% 0% / 50%)"])
		assert.throws(() => color(value), /opacity is not supported/, value);
	assert.throws(() => color("rgb(from red r g b)"), /unsupported color: rgb\(from red r g b\)/);
});

test("given a border shorthand with rgb() spaces, when normalizing, then keeps the function as one token", () => {
	const s = style({ border: "1pt solid rgb(0, 0, 255)" });
	assert.equal(s.borderColor, "rgb(0, 0, 255)");
	assert.deepEqual(color(s.borderColor), [0, 0, 255]);
});

test("given the flex shorthand, when normalizing, then keeps only the grow factor", () => {
	assert.equal(style({ flex: 1 }).flex, 1);
	assert.equal(style({ flex: "1 1 0%" }).flex, 1);
	assert.equal(style({ flex: "2 0 auto" }).flex, 2);
	assert.equal(style({ flex: "none" }).flex, 0);
	assert.throws(() => style({ flex: "10px" }), /invalid flex: 10px/);
});

test("given a font-family list, when selecting a font, then uses the first registered family or a generic fallback", () => {
	const inter = { regular: 2, bold: 3 };
	const fonts = new Map([
		["Helvetica", helvetica],
		["Inter", inter],
	]);
	const pick = (fontFamily) => font({ fontFamily }, helvetica, false, fonts).family;
	assert.equal(pick("'Missing', \"Inter\", sans-serif"), inter);
	assert.equal(pick("Missing, system-ui, sans-serif"), helvetica);
	assert.equal(pick("Arial"), helvetica);
	assert.throws(() => pick("Missing, monospace"), /unregistered font family: Missing, monospace/);
});
