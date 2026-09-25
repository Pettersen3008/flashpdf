import { fileURLToPath } from "node:url";

import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
	base: process.env.GITHUB_PAGES ? "/flashpdf/" : "/",
	plugins: [tailwindcss()],
	resolve: { alias: { "@": fileURLToPath(new URL(".", import.meta.url)) } },
});
