import assert from "node:assert/strict";
import { test } from "node:test";
import { point, edges, color } from "../dist/style.js";

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
