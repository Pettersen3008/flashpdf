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
const WINANSI = new RegExp(
	// oxlint-disable-next-line no-control-regex -- Reject characters outside WinAnsi.
	"[^\\u0009-\\u000d\\u0020-\\u007e\\u00a0-\\u00ff\\u20ac\\u201a\\u0192\\u201e\\u2026\\u2020\\u2021\\u02c6\\u2030\\u0160\\u2039\\u0152\\u017d\\u2018\\u2019\\u201c\\u201d\\u2022\\u2013\\u2014\\u02dc\\u2122\\u0161\\u203a\\u0153\\u017e\\u0178]",
	"u",
);
export function text(value: unknown): string {
	if (typeof value !== "string" && (typeof value !== "number" || !Number.isFinite(value)))
		throw new Error("expected text");
	const result = String(value);
	if (result.length > 65536) throw new Error("record too large");
	if (WINANSI.test(result)) throw new Error("unsupported WinAnsi character");
	return result;
}
