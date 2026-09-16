import assert from "node:assert/strict";
import { test } from "node:test";
import { render, stylesheet } from "../dist/index.js";
import { jsx, jsxs } from "../dist/jsx-runtime.js";

const string = (pdf) => Buffer.from(pdf).toString("latin1");

test("given CSS outside the supported subset, when parsing a stylesheet, then rejects it with its source position", () => {
  for (const [source, message] of [
    [
      ".a { line-height: 1.5 }",
      /line-height \(line height is fixed to the font size\).*"\.a" at 1:1/,
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
  ]) {
    assert.throws(() => stylesheet(source), message, source);
  }
});

test("given an unsupported inline style, when rendering, then rejects it with the same reason and the tag", async () => {
  for (const [style, message] of [
    [{ height: 40 }, /height \(block height comes from content\) on <main>/],
    [{ borderRadius: 4 }, /border-?[Rr]adius \(box corners are square\) on <main>/],
    [{ zoom: 2 }, /unsupported style property: zoom on <main>/],
    [{ display: "grid" }, /invalid display/],
  ]) {
    await assert.rejects(
      render(jsx("main", { style, children: "x" })),
      message,
      JSON.stringify(style),
    );
  }
});

test("given width or flex outside a flex row, when rendering, then rejects instead of dropping it", async () => {
  await assert.rejects(
    render(jsx("main", { style: { width: "40pt" }, children: "x" })),
    /width applies only to a flex row child, not <main>/,
  );
  await assert.rejects(
    render(jsx("main", { children: jsx("p", { style: { flex: 1 }, children: "x" }) })),
    /flex applies only to a flex row child, not <p>/,
  );
  const row = jsxs("div", {
    style: { display: "flex", flexDirection: "row" },
    children: [
      jsx("span", { style: { width: "40pt" }, children: "a" }),
      jsx("span", { style: { flex: 1 }, children: "b" }),
    ],
  });
  assert.ok((await render(row)).length > 0);
});

test("given two stylesheets of equal specificity, when rendering, then the later one wins", async () => {
  const document = jsx("main", { className: "ink", children: "Total" });
  const pdf = await render(document, {
    stylesheets: [".ink { color: #ff0000 }", ".ink { color: #0000ff }"],
  });
  assert.match(string(pdf), /0 0 1 rg/);
  assert.doesNotMatch(string(pdf), /1 0 0 rg/);
});
