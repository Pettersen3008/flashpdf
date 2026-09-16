import { defineConfig } from "vitest/config";

export default defineConfig({
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
				},
			},
		],
	},
});
