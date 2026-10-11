import assert from "node:assert/strict";

import { test } from "vitest";

import { stylesheet } from "../dist/index.js";
import { pageSetup } from "../dist/page.js";
import { point } from "../dist/style.js";

const css = (...sheets) => pageSetup(undefined, undefined, undefined, sheets);

test("given each CSS size form, when resolving @page, then sizes the page in points", () => {
	for (const [size, expected] of [
		["auto", [595, 842]],
		["landscape", [842, 595]],
		["portrait LEGAL", [612, 1008]],
		["A3 landscape", [1191, 842]],
		["148mm 210mm", [point("148mm"), point("210mm")]],
		["5in", [360, 360]],
	]) {
		const { width, height } = css(`@page { size: ${size} }`);
		assert.deepEqual([width, height], expected, size);
	}
});

test("given @page rules across stylesheets, when resolving the page, then later declarations win and !important beats both", () => {
	const page = css(
		"@page { size: A5; margin: 10mm }",
		"@page { margin-top: 1in !important } @PAGE { margin-top: 0; margin-left: 2cm }",
	);
	assert.deepEqual(page, {
		width: 420,
		height: 595,
		margin: [72, point("10mm"), point("10mm"), point("2cm")],
	});
});

test("given render options and @page, when resolving the page, then each option replaces its @page value and orientation turns either size", () => {
	const sheets = ["@page { size: A3; margin: 1in }"];
	assert.deepEqual(pageSetup(undefined, "landscape", undefined, sheets), {
		width: 1191,
		height: 842,
		margin: [72, 72, 72, 72],
	});
	assert.deepEqual(
		pageSetup({ width: "100mm", height: 200 }, "landscape", "20mm 15mm", sheets).margin,
		[point("20mm"), point("15mm"), point("20mm"), point("15mm")],
	);
	assert.deepEqual(
		pageSetup("Letter", undefined, { top: 10, right: "1cm", bottom: 0, left: "2pc" }, sheets),
		{ width: 612, height: 792, margin: [10, point("1cm"), 0, 24] },
	);
});

test("given page setup outside the supported subset, when validating, then rejects with a reason", () => {
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
	for (const [options, message] of [
		[["A4", "sideways"], /invalid orientation: sideways/],
		[[{ width: "0mm", height: 100 }], /invalid page width: 0mm/],
		[["A4", undefined, { top: 10 }], /invalid margin right: undefined/],
		[["A5", "landscape", "0 300pt"], /the margins leave no room for content/],
	])
		assert.throws(() => pageSetup(...options), message);
});
