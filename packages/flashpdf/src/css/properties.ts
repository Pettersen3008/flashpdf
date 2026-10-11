import type { Style } from "../element.js";

export type RejectedProperty =
	| "minWidth"
	| "maxWidth"
	| "borderRadius"
	| "letterSpacing"
	| "verticalAlign"
	| "objectFit"
	| "borderCollapse"
	| "borderSpacing";
type Side = "Top" | "Right" | "Bottom" | "Left";
/** CSS names with no `Style` key: `border-style`, the logical shorthands, which normalize() expands
 *  for horizontal left-to-right text, and the per-side border longhands, which it writes. */
export type CssProperty =
	| "borderStyle"
	| `${"margin" | "padding"}${"Inline" | "Block"}`
	| `border${Side}${"Width" | "Color" | "Style"}`;
type StyleProperty = Exclude<keyof Style, `--${string}`>;
type PropertyDefinition = { css?: string; inherited?: boolean; unsupported?: string };
const properties = {
	display: {},
	flexDirection: { css: "flex-direction" },
	flex: {},
	width: {},
	height: {},
	margin: {},
	marginTop: { css: "margin-top" },
	marginRight: { css: "margin-right" },
	marginBottom: { css: "margin-bottom" },
	marginLeft: { css: "margin-left" },
	padding: {},
	paddingTop: { css: "padding-top" },
	paddingRight: { css: "padding-right" },
	paddingBottom: { css: "padding-bottom" },
	paddingLeft: { css: "padding-left" },
	border: {},
	borderWidth: { css: "border-width" },
	borderColor: { css: "border-color" },
	borderTop: { css: "border-top" },
	borderRight: { css: "border-right" },
	borderBottom: { css: "border-bottom" },
	borderLeft: { css: "border-left" },
	borderStyle: { css: "border-style" },
	borderTopWidth: { css: "border-top-width" },
	borderRightWidth: { css: "border-right-width" },
	borderBottomWidth: { css: "border-bottom-width" },
	borderLeftWidth: { css: "border-left-width" },
	borderTopColor: { css: "border-top-color" },
	borderRightColor: { css: "border-right-color" },
	borderBottomColor: { css: "border-bottom-color" },
	borderLeftColor: { css: "border-left-color" },
	borderTopStyle: { css: "border-top-style" },
	borderRightStyle: { css: "border-right-style" },
	borderBottomStyle: { css: "border-bottom-style" },
	borderLeftStyle: { css: "border-left-style" },
	marginInline: { css: "margin-inline" },
	marginBlock: { css: "margin-block" },
	paddingInline: { css: "padding-inline" },
	paddingBlock: { css: "padding-block" },
	background: {},
	backgroundColor: { css: "background-color" },
	gap: {},
	fontSize: { css: "font-size", inherited: true },
	fontFamily: { css: "font-family", inherited: true },
	fontWeight: { css: "font-weight", inherited: true },
	textAlign: { css: "text-align", inherited: true },
	// CSS propagates decoration to descendants rather than inheriting it; the effect is the same here.
	textDecoration: { css: "text-decoration", inherited: true },
	color: { inherited: true },
	// Inherits through the compiler's text state: a factor stays a factor, a length stays a length.
	lineHeight: { css: "line-height" },
	breakBefore: { css: "break-before" },
	breakAfter: { css: "break-after" },
	breakInside: { css: "break-inside" },
	minWidth: {
		css: "min-width",
		unsupported: "only width on a flex row child is implemented",
	},
	maxWidth: {
		css: "max-width",
		unsupported: "only width on a flex row child is implemented",
	},
	borderRadius: { css: "border-radius", unsupported: "box corners are square" },
	letterSpacing: {
		css: "letter-spacing",
		unsupported: "text shaping is not implemented",
	},
	objectFit: {
		css: "object-fit",
		unsupported: "size an image with width and height; it always scales to fill them",
	},
	verticalAlign: {
		css: "vertical-align",
		unsupported: "cells align to the top; middle and bottom are not implemented",
	},
	borderCollapse: {
		css: "border-collapse",
		unsupported:
			"cell borders are separate and adjacent ones double up; draw one edge per cell, such as border-bottom",
	},
	borderSpacing: {
		css: "border-spacing",
		unsupported: "cells tile their row; use cell padding",
	},
} as const satisfies Record<StyleProperty | RejectedProperty | CssProperty, PropertyDefinition>;

type PropertyName = keyof typeof properties;
const propertyEntries = Object.entries(properties) as [PropertyName, PropertyDefinition][];
const cssNames = new Map<string, PropertyName>(
	propertyEntries.map(([name, definition]) => [definition.css ?? name, name]),
);
// Tailwind's `underline` and `line-through`; text-decoration here takes only line values anyway.
cssNames.set("text-decoration-line", "textDecoration");
// Horizontal left-to-right text: inline runs from left to right, block from top to bottom.
for (const box of ["margin", "padding"] as const)
	for (const [logical, side] of [
		["inline-start", "Left"],
		["inline-end", "Right"],
		["block-start", "Top"],
		["block-end", "Bottom"],
	] as const)
		cssNames.set(`${box}-${logical}`, `${box}${side}`);
export const inherited = new Set<string>(
	propertyEntries.filter(([, definition]) => definition.inherited).map(([name]) => name),
);
export const supported = new Set<keyof Style>(
	propertyEntries
		.filter(([, definition]) => !definition.unsupported)
		.map(([name]) => name) as (keyof Style)[],
);
// No prototype, so a style key such as `toString` finds no inherited "reason".
export const unsupported = Object.assign(
	Object.create(null),
	Object.fromEntries(
		propertyEntries
			.filter(([, definition]) => definition.unsupported)
			.map(([name, definition]) => [name, definition.unsupported]),
	),
) as Partial<Record<keyof Style | RejectedProperty | CssProperty, string>>;

export function property(
	name: string,
	context = "",
): keyof Style | RejectedProperty | CssProperty | `--${string}` {
	if (name.startsWith("--")) return name as `--${string}`;
	// CSS property names are ASCII case-insensitive; camelCase is the JSX spelling, not CSS.
	const result = cssNames.get(name.toLowerCase());
	if (result === undefined) {
		const kebab = name.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);
		const hint = cssNames.has(kebab) ? ` (use kebab-case: ${kebab})` : "";
		throw new Error(`unsupported CSS property: ${name}${hint}${context}`);
	}
	const reason = unsupported[result];
	if (reason) throw new Error(`unsupported CSS property: ${name} (${reason})${context}`);
	return result;
}
