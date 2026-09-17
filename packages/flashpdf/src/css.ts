import { parse } from "./css/parser.js";

export { property, supported, unsupported } from "./css/properties.js";
export { resolveStyles } from "./css/cascade.js";
export type { StyledNode } from "./css/cascade.js";

export function stylesheet(source: string): string {
	parse(source);
	return source;
}
