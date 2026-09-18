import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { render } from "../../dist/index.js";
import { jsx } from "../../dist/jsx-runtime.js";

const root = dirname(dirname(dirname(dirname(fileURLToPath(import.meta.url)))));
const work = mkdtempSync(join(tmpdir(), "flashpdf-tailwind-"));
try {
	const output = join(work, "tailwind.css");
	execFileSync(
		"pnpm",
		[
			"exec",
			"tailwindcss",
			"-i",
			fileURLToPath(new URL("./tailwind.css", import.meta.url)),
			"-o",
			output,
			"--minify",
		],
		{ cwd: root, stdio: "pipe" },
	);
	const pdf = await render(jsx("p", { className: "text-right", children: "Tailwind" }), {
		stylesheets: [readFileSync(output, "utf8")],
	});
	assert.equal(new TextDecoder().decode(pdf.subarray(0, 5)), "%PDF-");
} finally {
	rmSync(work, { recursive: true, force: true });
}
