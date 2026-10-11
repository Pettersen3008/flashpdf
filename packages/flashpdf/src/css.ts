import { parse, type PageDeclaration } from "./css/parser.js";
import { cssPage } from "./page.js";
import { boxStyle, color, style } from "./style.js";

export { resolveStyles } from "./css/cascade.js";
export type { StyledNode } from "./css/cascade.js";

/** Validates every declaration of every supported rule up front; `render` only validates rules that match. */
export function stylesheet(source: string | TemplateStringsArray, ...values: unknown[]): string {
	const css = typeof source === "string" ? source : String.raw(source, ...values);
	const pages: PageDeclaration[] = [];
	for (const rule of parse(css, pages))
		for (const declarations of Object.values(rule.declarations())) {
			// Custom properties and var() resolve only in an element's cascade, and Tailwind puts
			// calc() and color-mix() in most utilities, so render checks those where a rule applies.
			const known = Object.entries(declarations).filter(
				([key, value]) =>
					!key.startsWith("--") && !/\b(?:var|calc|min|max|clamp|color-mix)\(/i.test(String(value)),
			);
			try {
				const resolved = style(Object.fromEntries(known));
				boxStyle(resolved, 12);
				if (resolved.color !== undefined) color(resolved.color);
			} catch (error) {
				if (error instanceof Error) error.message += rule.where;
				throw error;
			}
		}
	cssPage(pages);
	return css;
}
