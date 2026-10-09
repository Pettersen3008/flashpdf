import { parse } from "./css/parser.js";
import { boxStyle, color, style } from "./style.js";

export { resolveStyles } from "./css/cascade.js";
export type { StyledNode } from "./css/cascade.js";

/** Validates every declaration of every supported rule up front; `render` only validates rules that match. */
export function stylesheet(source: string | TemplateStringsArray, ...values: unknown[]): string {
	const css = typeof source === "string" ? source : String.raw(source, ...values);
	for (const rule of parse(css))
		for (const declarations of Object.values(rule.declarations())) {
			// A var() value resolves only in an element's cascade, so render checks it.
			const known = Object.entries(declarations).filter(
				([key, value]) => !key.startsWith("--") && !String(value).includes("var("),
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
	return css;
}
