/** Sources `stylesheet()` declared, true once a render's full check passed. */
// ponytail: grows with every distinct declared source; bound it if apps build sheets per request.
export const strict = new Map<string, boolean>();

/** Marks the sheet for a full check: the first `render` that uses it validates every declaration of
 *  every supported rule, where it otherwise validates only rules that match. */
export function stylesheet(source: string | TemplateStringsArray, ...values: unknown[]): string {
	const css = typeof source === "string" ? source : String.raw(source, ...values);
	if (!strict.has(css)) strict.set(css, false);
	return css;
}
