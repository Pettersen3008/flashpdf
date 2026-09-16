import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { render } from "../dist/index.js";
import { jsx, jsxs } from "../dist/jsx-runtime.js";

const repo = new URL("../../../", import.meta.url);
const golden = new URL("tests/golden/css-invoice.txt", repo);

const invoice = {
  title: "Acme Supply Co.",
  number: "2026-0042",
  dueDate: "30 September 2026",
  items: Array.from({ length: 80 }, (_, index) => ({
    id: `${index}`,
    name: `Line item ${index + 1}`,
    total: `$${(index + 1) * 12}.00`,
  })),
};

/** The CSS invoice from examples/css-invoice.tsx, wired through the consumer's CSS build. */
function document_() {
  return jsxs("main", {
    className: "invoice",
    children: [
      jsxs("header", {
        className: "header",
        children: [
          jsxs("div", {
            children: [
              jsx("h1", { children: invoice.title }),
              jsx("p", { className: "muted", children: `Invoice #${invoice.number}` }),
            ],
          }),
          jsx("p", {
            style: { textAlign: "right", fontSize: 12 },
            children: `Due ${invoice.dueDate}`,
          }),
        ],
      }),
      jsx("section", {
        className: "items",
        children: invoice.items.map((item) =>
          jsxs(
            "div",
            {
              className: "item",
              children: [
                jsx("span", { children: item.name }),
                jsx("span", { className: "amount", children: item.total }),
              ],
            },
            item.id,
          ),
        ),
      }),
    ],
  });
}

// ponytail: the content stream is the drawing program, so this catches any
// position, colour, font or pagination change. Rasterise with pdfium instead if
// a future change can alter pixels without altering operators.
test("given the CSS invoice example, when rendering, then every page draws the golden content stream", async () => {
  const css = readFileSync(new URL("../examples/invoice.module.css", import.meta.url), "utf8");
  const pdf = await render(document_(), { pageFormat: "A4", margin: 36, stylesheets: [css] });
  assert.deepEqual(
    pdf,
    await render(document_(), { pageFormat: "A4", margin: 36, stylesheets: [css] }),
  );

  const path = new URL("target/css-invoice.pdf", repo);
  writeFileSync(path, pdf);
  const dumped = spawnSync(
    "cargo",
    [
      "run",
      "--locked",
      "-q",
      "-p",
      "flashpdf-core",
      "--example",
      "validate_pdf",
      "--",
      path.pathname,
      "2",
      "--operators",
    ],
    { cwd: repo, encoding: "utf8" },
  );
  assert.equal(dumped.status, 0, dumped.stderr);

  if (process.env.UPDATE_GOLDEN) writeFileSync(golden, dumped.stdout);
  assert.equal(
    dumped.stdout,
    readFileSync(golden, "utf8"),
    "content stream changed; rerun with UPDATE_GOLDEN=1 and review the diff",
  );
});
