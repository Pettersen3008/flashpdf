import assert from "node:assert/strict";

import { test } from "vitest";

import { boxSize, color, dimension, edges, font, helvetica, point, style } from "../dist/style.js";

test("given pt, px, em, and rem lengths, when resolving a point value, then converts to points", () => {
	assert.equal(point("12pt"), 12);
	assert.equal(point("16px"), 12);
	assert.equal(point("2em", 10), 20);
	assert.equal(point("2rem", 10), 24);
	assert.equal(point("0"), 0);
});

test("given mm, cm, in, pc, and Q lengths, when resolving a length or width, then converts at 72pt per inch", () => {
	for (const length of ["25.4mm", "2.54cm", "1in", "6pc", "101.6Q"]) {
		assert.equal(point(length), 72, length);
		assert.equal(dimension(length, "width").value, 72, length);
	}
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

test("given each flex shorthand form, when normalizing, then expands it to grow, shrink, and basis as CSS does", () => {
	const longhands = (flex) => {
		const { flexGrow, flexShrink, flexBasis } = style({ flex });
		return [flexGrow, flexShrink, flexBasis];
	};
	assert.deepEqual(longhands(1), [1, 1, 0]);
	assert.deepEqual(longhands("2 0"), [2, 0, 0]);
	assert.deepEqual(longhands("1 1 0"), [1, 1, "0"]);
	assert.deepEqual(longhands("auto"), [1, 1, "auto"]);
	assert.deepEqual(longhands("none"), [0, 0, "auto"]);
	assert.deepEqual(longhands("initial"), [0, 1, "auto"]);
	assert.deepEqual(longhands("100px"), [1, 1, "100px"]);
	assert.deepEqual(longhands("50% 2"), [2, 1, "50%"]);
	assert.throws(() => style({ flex: "1 2 3" }), /invalid flex-basis: 3/);
	assert.throws(() => style({ flex: "1 auto 2" }), /invalid flex: 1 auto 2/);
});

test("given a percent height, when sizing a box, then resolves it against the parent's fixed height or rejects without one", () => {
	const s = style({ height: "50%", width: "auto", maxWidth: "none", margin: "0 auto" });
	assert.deepEqual(boxSize(s, 200), {
		borderBox: false,
		autoMargin: 2 | 8,
		width: undefined,
		minWidth: undefined,
		maxWidth: undefined,
		height: 100,
		minHeight: undefined,
		maxHeight: undefined,
	});
	assert.throws(() => boxSize(s, undefined), /a percent height needs a parent with a fixed height/);
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

test("given a font weight outside normal, bold, and 1 to 1000, when normalizing, then rejects it", () => {
	for (const fontWeight of ["normal", "bold", 700, "700"]) style({ fontWeight });
	for (const fontWeight of ["bolder", "boldd", 0, "1001"])
		assert.throws(() => style({ fontWeight }), /invalid fontWeight/, String(fontWeight));
});

test("given a rem width, as minified Tailwind emits, when parsing it, then converts at 12pt per rem", () => {
	assert.deepEqual(dimension("8rem", "width"), { kind: 1, value: 96 });
});

test("given a width with more digits than a float holds, when normalizing, then rejects it", () => {
	assert.throws(() => style({ width: `${"9".repeat(45)}pt` }), /invalid width: 9+pt/);
});

test("given signed and percentage position offsets, when normalizing, then preserves finite values and rejects invalid input", () => {
	assert.deepEqual(style({ position: "absolute", top: "-2mm", left: "50%" }), {
		position: "absolute",
		top: "-2mm",
		left: "50%",
	});
	for (const value of [Infinity, "NaNpt", "1vh", "calc(1pt)"])
		assert.throws(() => style({ top: value }), /invalid/);
	assert.throws(() => style({ position: "sticky" }), /invalid position/);
});
