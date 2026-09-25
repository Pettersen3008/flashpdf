export function object(value: unknown): Record<string, unknown> {
	if (typeof value !== "object" || value === null || Array.isArray(value))
		throw new Error("expected element or props");
	return value as Record<string, unknown>;
}
export function props(value: unknown, allowed: string[]) {
	const result = object(value);
	for (const key of Object.keys(result))
		if (key !== "key" && !allowed.includes(key)) throw new Error(`unsupported property: ${key}`);
	return result;
}
export function number(value: unknown, positive = false): number {
	if (
		typeof value !== "number" ||
		!Number.isFinite(Math.fround(value)) ||
		(positive ? Math.fround(value) <= 0 : value < 0)
	)
		throw new Error("invalid number");
	return Math.fround(value);
}
/** Control characters other than ASCII whitespace and the page tokens, plus lone surrogates
 *  (paired ones form a single code point under the `u` flag). Whether a printable character
 *  has a glyph is the font's decision at layout. */
// oxlint-disable-next-line no-control-regex -- Control characters are the point.
const REJECTED = /[\u0000-\u0008\u000b\u000e-\u001d\u007f-\u009f\ud800-\udfff]/u;
export function text(value: unknown): string {
	if (typeof value !== "string" && (typeof value !== "number" || !Number.isFinite(value)))
		throw new Error("expected text");
	const result = String(value);
	if (result.length > 65536) throw new Error("record too large");
	if (REJECTED.test(result)) throw new Error("unsupported control character");
	return result;
}
