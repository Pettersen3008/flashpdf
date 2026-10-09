import { defineConfig } from "vitest/config";

export default defineConfig({
	root: import.meta.dirname,
	test: {
		projects: [
			{
				test: {
					name: "unit",
					include: ["packages/flashpdf/test/*.unit.test.mjs"],
				},
			},
			{
				test: {
					name: "integration",
					include: [
						"packages/flashpdf/test/*.integration.test.mjs",
						"tests/*.integration.test.mjs",
					],
					testTimeout: 30_000,
					// The CSS invoice example imports one module twice, as classes and as ?inline text; scoped names would hash differently.
					css: { include: /\.css/, modules: { classNameStrategy: "non-scoped" } },
				},
			},
		],
	},
});
