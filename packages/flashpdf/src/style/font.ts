import type { FontSlots, NormalizedStyle } from "./types.js";

export function bold(value: unknown): boolean {
	return (
		value === "bold" ||
		(typeof value === "number" && value >= 600) ||
		(typeof value === "string" && Number(value) >= 600)
	);
}

export const helvetica: FontSlots = { regular: 0, bold: 1 };

export function font(
	s: NormalizedStyle,
	inherited: FontSlots,
	inheritedBold: boolean,
	fonts: ReadonlyMap<string, FontSlots>,
) {
	const requested = s.fontFamily;
	const family =
		requested === undefined
			? inherited
			: typeof requested === "string"
				? fonts.get(requested.trim())
				: undefined;
	if (!family) throw new Error(`unregistered font family: ${requested}`);
	const weight = s.fontWeight === undefined ? inheritedBold : bold(s.fontWeight);
	if (weight && family.bold === undefined)
		throw new Error(`font family has no bold face: ${s.fontFamily ?? "inherited"}`);
	return { family, weight };
}
