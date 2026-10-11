import assert from "node:assert/strict";

import { test } from "vitest";

import {
	color,
	counter,
	courier,
	edges,
	font,
	helvetica,
	point,
	style,
	times,
} from "../dist/style.js";

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
	assert.deepEqual(edges(style({ margin: "4pt" }), "margin", 12), [4, 4, 4, 4]);
	assert.deepEqual(edges(style({ margin: "4pt 8pt" }), "margin", 12), [4, 8, 4, 8]);
	assert.deepEqual(edges(style({ margin: "4pt 8pt 12pt" }), "margin", 12), [4, 8, 12, 8]);
	assert.deepEqual(edges(style({ margin: "1pt 2pt 3pt 4pt" }), "margin", 12), [1, 2, 3, 4]);
});

test("given a per-side override, when expanding margin edges, then it wins over the shorthand", () => {
	assert.deepEqual(edges(style({ margin: "4pt", marginTop: "9pt" }), "margin", 12), [9, 4, 4, 4]);
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
	assert.equal(s.borderTopColor, "rgb(0, 0, 255)");
	assert.deepEqual(color(s.borderTopColor), [0, 0, 255]);
});

test("given the flex shorthand, when normalizing, then keeps only the grow factor", () => {
	assert.equal(style({ flex: 1 }).flex, 1);
	assert.equal(style({ flex: "1 1 0%" }).flex, 1);
	assert.equal(style({ flex: "2 0 auto" }).flex, 2);
	assert.equal(style({ flex: "none" }).flex, 0);
	assert.throws(() => style({ flex: "10px" }), /invalid flex: 10px/);
});

test("given a font-family list, when selecting a font, then takes the first registered family or built-in match in order", () => {
	const inter = { regular: 2, bold: 3 };
	const fonts = new Map([
		["Helvetica", helvetica],
		["Inter", inter],
	]);
	const pick = (fontFamily, inherited = helvetica) =>
		font({ fontFamily }, inherited, false, false, fonts).family;
	assert.equal(pick("'Missing', \"Inter\", sans-serif"), inter);
	assert.equal(pick("Missing, system-ui, sans-serif"), helvetica);
	assert.equal(pick("Arial"), helvetica);
	assert.equal(pick("Georgia, 'Times New Roman', serif"), times);
	assert.equal(pick("Missing, monospace"), courier);
	assert.equal(pick("Courier New, serif"), courier);
	assert.equal(pick("Inter, serif"), inter);
	// CSS reads a quoted generic keyword as a family name, so it matches nothing here.
	assert.equal(pick("'serif', monospace"), courier);
});

test("given a font-family list with no match, when selecting a font, then rejects naming the families, and inherits when there is no list", () => {
	const fonts = new Map([["Helvetica", helvetica]]);
	assert.throws(
		() => font({ fontFamily: "Missing, 'Also Missing', cursive" }, times, false, false, fonts),
		/unregistered font family: Missing, 'Also Missing', cursive \(register it in fonts/,
	);
	assert.throws(
		() => font({ fontFamily: "'serif'" }, times, false, false, fonts),
		/unregistered font family: 'serif'/,
	);
	assert.equal(font({}, courier, false, false, fonts).family, courier);
});

test("given a font weight outside normal, bold, and 1 to 1000, when normalizing, then rejects it", () => {
	for (const fontWeight of ["normal", "bold", 700, "700"]) style({ fontWeight });
	for (const fontWeight of ["bolder", "boldd", 0, "1001"])
		assert.throws(() => style({ fontWeight }), /invalid fontWeight/, String(fontWeight));
});

test("given a width with more digits than a float holds, when normalizing, then rejects it", () => {
	assert.throws(() => style({ width: `${"9".repeat(45)}pt` }), /invalid width: 9+pt/);
});

test("given an oblique angle, a percent spacing, a wavy line, or another transform, when normalizing, then rejects naming the value", () => {
	for (const [value, message] of [
		[{ fontStyle: "oblique 10deg" }, /invalid fontStyle: oblique 10deg/],
		[{ letterSpacing: "10%" }, /invalid letterSpacing: 10%/],
		[{ wordSpacing: "20000pt" }, /invalid wordSpacing: 20000pt/],
		[{ textDecoration: "underline wavy" }, /text-decoration: wavy is not supported/],
		[{ textTransform: "full-width" }, /invalid textTransform: full-width/],
	])
		assert.throws(() => style(value), message);
});

test("given list-style, when normalizing, then omitted parts reset and unsupported parts reject", () => {
	assert.deepEqual(
		[
			style({ listStyle: "inside" }).listStyleType,
			style({ listStyle: "inside" }).listStylePosition,
		],
		["disc", "inside"],
	);
	assert.equal(style({ listStyleType: "square", listStyle: "none" }).listStyleType, "none");
	assert.equal(style({ listStyle: "none upper-roman" }).listStyleType, "upper-roman");
	assert.throws(() => style({ listStyle: "url(x.png)" }), /list-style-image is not supported/);
	assert.throws(
		() => style({ listStyleType: "lower-greek" }),
		/invalid listStyleType: lower-greek/,
	);
	assert.throws(() => style({ listStylePosition: "middle" }), /invalid listStylePosition/);
	assert.throws(() => style({ listStyleImage: "none" }), /listStyleImage/);
});

test("given ordinals outside a counter's range, when numbering, then letters and numerals count and the rest fall back to decimal", () => {
	assert.deepEqual(
		[1, 26, 27, 703].map((n) => counter("lower-alpha", n)),
		["a", "z", "aa", "aaa"],
	);
	assert.deepEqual(
		[4, 9, 1994, 3999].map((n) => counter("upper-roman", n)),
		["IV", "IX", "MCMXCIV", "MMMCMXCIX"],
	);
	assert.deepEqual(
		[0, -3, 4000].map((n) => [counter("lower-alpha", n), counter("lower-roman", n)]),
		[
			["0", "0"],
			["-3", "-3"],
			["ewv", "4000"],
		],
	);
	assert.deepEqual(
		[7, 0, -5, 123].map((n) => counter("decimal-leading-zero", n)),
		["07", "00", "-05", "123"],
	);
});

test("given white-space and vertical-align values, when normalizing, then only the supported keywords pass", () => {
	for (const whiteSpace of ["normal", "nowrap", "pre", "pre-wrap", "pre-line"])
		style({ whiteSpace });
	assert.throws(() => style({ whiteSpace: "break-spaces" }), /invalid whiteSpace: break-spaces/);
	for (const verticalAlign of ["baseline", "sub", "super"]) style({ verticalAlign });
	assert.throws(() => style({ verticalAlign: "top" }), /invalid verticalAlign: top/);
	assert.throws(() => style({ verticalAlign: "toString" }), /invalid verticalAlign/);
});
