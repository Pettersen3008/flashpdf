import assert from "node:assert/strict";

import { test } from "vitest";

import { stylesheet } from "../dist/index.js";

test("given @page outside the supported subset, when validating a stylesheet, then rejects with a reason", () => {
	for (const [source, message] of [
		["@page :first { margin: 0 }", /unsupported @page selector: :first .*at 1:1/],
		["@page { @top-center { content: 'x' } }", /unsupported @page margin box: @top-center/],
		["@page { marks: crop }", /unsupported @page property: marks/],
		["@page { size: B5 }", /unsupported page size: B5/],
		["@page { size: A4 10mm }", /invalid page size: A4 10mm/],
		["@page { size: 15000pt 10in }", /invalid page size: 15000pt/],
		["@page { margin: 10% }", /invalid margin: 10% in @page at 1:1/],
	])
		assert.throws(() => stylesheet(source), message, source);
});
