import { parse } from "./css/parser.js";

export { property, supported, unsupported } from "./css/properties.js";
export { resolveStyles } from "./css/cascade.js";
export type { StyledNode } from "./css/cascade.js";

/** Validates every declaration of every supported rule up front; `render` only validates rules that match. */
export function stylesheet(source: string | TemplateStringsArray, ...values: unknown[]): string {
	const css = typeof source === "string" ? source : String.raw(source, ...values);
	for (const rule of parse(css)) rule.declarations();
	return css;
}
