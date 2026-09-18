import assert from "node:assert/strict";

import { transformSync } from "@babel/core";
import stylexPlugin from "@stylexjs/babel-plugin";

import { render } from "../../dist/index.js";
import { jsx } from "../../dist/jsx-runtime.js";

const source = `
import * as stylex from "@stylexjs/stylex";
const styles = stylex.create({ total: { color: "#123456", textAlign: "right" } });
export const className = stylex.props(styles.total).className;
`;
const result = transformSync(source, {
	filename: "invoice.stylex.js",
	plugins: [[stylexPlugin, { dev: false }]],
});
const rules = result?.metadata?.stylex;
if (!Array.isArray(rules)) throw new Error("StyleX emitted no CSS");
const css = stylexPlugin.processStylexRules(rules);
const className = rules.map(([name]) => name).join(" ");
const pdf = await render(jsx("p", { className, children: "StyleX" }), {
	stylesheets: [css],
});
assert.equal(new TextDecoder().decode(pdf.subarray(0, 5)), "%PDF-");
