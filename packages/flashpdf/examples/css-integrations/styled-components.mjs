import assert from "node:assert/strict";

import React from "react";
import { renderToString } from "react-dom/server";
import { ServerStyleSheet, styled } from "styled-components";

import { render } from "../../dist/index.js";
import { jsx } from "../../dist/jsx-runtime.js";

const Total = styled.p`
	color: #123456;
	text-align: right;
`;
const sheet = new ServerStyleSheet();
try {
	const html = renderToString(sheet.collectStyles(React.createElement(Total, null, "Total")));
	const className = html.match(/class="[^"]* ([^"]+)"/)?.[1];
	const extracted = sheet.getStyleElement()[0]?.props.dangerouslySetInnerHTML.__html;
	if (!className || typeof extracted !== "string")
		throw new Error("styled-components emitted no CSS");
	const css = extracted.match(new RegExp(`\\.${className}\\{[^}]+\\}`))?.[0];
	if (!css) throw new Error("styled-components emitted no component rule");
	const pdf = await render(jsx("p", { className, children: "styled-components" }), {
		stylesheets: [css],
	});
	assert.equal(new TextDecoder().decode(pdf.subarray(0, 5)), "%PDF-");
} finally {
	sheet.seal();
}
