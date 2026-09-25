import type { FontSlots, NormalizedStyle } from "./types.js";

export function bold(value: unknown): boolean {
	return (
		value === "bold" ||
		(typeof value === "number" && value >= 600) ||
		(typeof value === "string" && Number(value) >= 600)
	);
}

export const helvetica: FontSlots = { regular: 0, bold: 1 };

// Generic families and the two names browsers map to Helvetica anyway fall back to the built-in face.
const GENERIC = new Set([
	"helvetica",
	"arial",
	"sans-serif",
	"serif",
	"system-ui",
	"ui-sans-serif",
	"ui-serif",
]);

export function families(value: string): string[] {
	return value
		.split(",")
		.map((name) =>
			name
				.trim()
				.replace(/^(["'])(.*)\1$/, "$2")
				.trim(),
		)
		.filter(Boolean);
}

export function font(
	s: NormalizedStyle,
	inherited: FontSlots,
	inheritedBold: boolean,
	fonts: ReadonlyMap<string, FontSlots>,
) {
	const requested = s.fontFamily;
	let family: FontSlots | undefined = inherited;
	if (requested !== undefined) {
		const names = typeof requested === "string" ? families(requested) : [];
		family =
			names.map((name) => fonts.get(name)).find(Boolean) ??
			(names.some((name) => GENERIC.has(name.toLowerCase())) ? helvetica : undefined);
		if (!family) throw new Error(`unregistered font family: ${String(requested)}`);
	}
	const weight = s.fontWeight === undefined ? inheritedBold : bold(s.fontWeight);
	if (weight && family.bold === undefined)
		throw new Error(`font family has no bold face: ${s.fontFamily ?? "inherited"}`);
	return { family, weight };
}
