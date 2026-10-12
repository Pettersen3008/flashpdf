import type { FontSlots, NormalizedStyle } from "./types.js";

export function bold(value: unknown): boolean {
	if (value === "normal" || value === "bold") return value === "bold";
	const weight =
		typeof value === "string" && /^\d+(?:\.\d+)?$/.test(value.trim()) ? Number(value) : value;
	if (typeof weight !== "number" || !(weight >= 1 && weight <= 1000))
		throw new Error(`invalid fontWeight: ${String(value)} (use normal, bold, or 1 to 1000)`);
	return weight >= 600;
}

export const helvetica: FontSlots = { regular: 0, bold: 1, italic: 2, boldItalic: 3 };
export const times: FontSlots = { regular: 4, bold: 5, italic: 6, boldItalic: 7 };
export const courier: FontSlots = { regular: 8, bold: 9, italic: 10, boldItalic: 11 };

export function italic(value: unknown): boolean {
	if (value !== "normal" && value !== "italic" && value !== "oblique")
		throw new Error(`invalid fontStyle: ${String(value)} (use normal, italic, or oblique)`);
	return value !== "normal";
}

/** The face slot for a weight and style; `oblique` asks the engine to slant an upright face. */
export function face(family: FontSlots, bold: boolean, slanted: boolean) {
	const own = slanted ? (bold ? family.boldItalic : family.italic) : undefined;
	if (own !== undefined) return { font: own, oblique: false };
	return { font: bold ? family.bold! : family.regular, oblique: slanted };
}

// Built-in families by name, and by generic keyword, which CSS recognises only unquoted.
const NAMED = new Map([
	["helvetica", helvetica],
	["arial", helvetica],
	["times", times],
	["times new roman", times],
	["courier", courier],
	["courier new", courier],
]);
const GENERIC = new Map([
	["sans-serif", helvetica],
	["system-ui", helvetica],
	["ui-sans-serif", helvetica],
	["serif", times],
	["ui-serif", times],
	["monospace", courier],
	["ui-monospace", courier],
]);

export function families(value: string): { name: string; quoted: boolean }[] {
	return value
		.split(",")
		.map((token) => {
			const name = token.trim();
			const quoted = /^(["']).*\1$/.test(name);
			return { name: (quoted ? name.slice(1, -1) : name).trim(), quoted };
		})
		.filter(({ name }) => name);
}

export function font(
	s: NormalizedStyle,
	inherited: FontSlots,
	inheritedBold: boolean,
	inheritedItalic: boolean,
	fonts: ReadonlyMap<string, FontSlots>,
) {
	let family = inherited;
	if (s.fontFamily !== undefined) {
		const names = typeof s.fontFamily === "string" ? families(s.fontFamily) : [];
		// An entry that matches nothing is skipped for a later one; a list with no match rejects.
		const found = names
			.map(({ name, quoted }) => {
				const key = name.toLowerCase().replace(/\s+/g, " ");
				return fonts.get(name) ?? NAMED.get(key) ?? (quoted ? undefined : GENERIC.get(key));
			})
			.find(Boolean);
		if (!found)
			throw new Error(
				`unregistered font family: ${String(s.fontFamily)} (register it in fonts, or end the list with serif, sans-serif, or monospace)`,
			);
		family = found;
	}
	const weight = s.fontWeight === undefined ? inheritedBold : bold(s.fontWeight);
	const slanted = s.fontStyle === undefined ? inheritedItalic : italic(s.fontStyle);
	if (weight && face(family, true, slanted).font === undefined)
		throw new Error(`font family has no bold face: ${s.fontFamily ?? "inherited"}`);
	return { family, weight, italic: slanted };
}
