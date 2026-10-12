import type { Style } from "../element.js";

export type RejectedProperty =
	| "boxShadow"
	| "outline"
	| "outlineColor"
	| "outlineOffset"
	| "outlineStyle"
	| "outlineWidth"
	| "borderImage"
	| "backgroundImage"
	| "listStyleImage"
	| "objectFit"
	| "boxDecorationBreak";
type Side = "Top" | "Right" | "Bottom" | "Left";
/** CSS names with no `Style` key: `border-style`, the logical shorthands, which normalize() expands
 *  for horizontal left-to-right text, and the per-side border longhands, which it writes. */
export type CssProperty =
	| "borderStyle"
	| `${"margin" | "padding"}${"Inline" | "Block"}`
	| `border${Side}${"Width" | "Color" | "Style"}`;
type StyleProperty = Exclude<keyof Style, `--${string}`>;
type PropertyDefinition = { css?: string; inherited?: boolean; unsupported?: string };
const NO_OUTLINE = "outlines are not drawn; use a border";
const properties = {
	display: {},
	flexDirection: { css: "flex-direction" },
	flexWrap: { css: "flex-wrap" },
	justifyContent: { css: "justify-content" },
	alignItems: { css: "align-items" },
	alignSelf: { css: "align-self" },
	flex: {},
	flexGrow: { css: "flex-grow" },
	flexShrink: { css: "flex-shrink" },
	flexBasis: { css: "flex-basis" },
	width: {},
	minWidth: { css: "min-width" },
	maxWidth: { css: "max-width" },
	height: {},
	minHeight: { css: "min-height" },
	maxHeight: { css: "max-height" },
	boxSizing: { css: "box-sizing" },
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
	borderRadius: { css: "border-radius" },
	borderTopLeftRadius: { css: "border-top-left-radius" },
	borderTopRightRadius: { css: "border-top-right-radius" },
	borderBottomRightRadius: { css: "border-bottom-right-radius" },
	borderBottomLeftRadius: { css: "border-bottom-left-radius" },
	// Not inherited: the compiler multiplies it down the tree.
	opacity: {},
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
	fontStyle: { css: "font-style", inherited: true },
	fontKerning: { css: "font-kerning", inherited: true },
	textAlign: { css: "text-align", inherited: true },
	textTransform: { css: "text-transform", inherited: true },
	// CSS propagates decoration to descendants rather than inheriting it; the effect is the same here.
	textDecoration: { css: "text-decoration", inherited: true },
	textDecorationColor: { css: "text-decoration-color", inherited: true },
	whiteSpace: { css: "white-space", inherited: true },
	// Not inherited: the compiler accumulates the shift down nested inline elements.
	verticalAlign: { css: "vertical-align" },
	// Inherited so a child sees the shorthand in its cascade order, which decides against longhands.
	listStyle: { css: "list-style", inherited: true },
	listStyleType: { css: "list-style-type", inherited: true },
	listStylePosition: { css: "list-style-position", inherited: true },
	color: { inherited: true },
	// Inherits through the compiler's text state: a factor stays a factor, a length stays a length.
	lineHeight: { css: "line-height" },
	// Inherit through the text state too, so `em` resolves once, against the declaring element.
	letterSpacing: { css: "letter-spacing" },
	wordSpacing: { css: "word-spacing" },
	breakBefore: { css: "break-before" },
	breakAfter: { css: "break-after" },
	breakInside: { css: "break-inside" },
	boxShadow: {
		css: "box-shadow",
		unsupported: "shadows are not drawn; use a border or a background",
	},
	outline: { css: "outline", unsupported: NO_OUTLINE },
	outlineColor: { css: "outline-color", unsupported: NO_OUTLINE },
	outlineOffset: { css: "outline-offset", unsupported: NO_OUTLINE },
	outlineStyle: { css: "outline-style", unsupported: NO_OUTLINE },
	outlineWidth: { css: "outline-width", unsupported: NO_OUTLINE },
	borderImage: {
		css: "border-image",
		unsupported: "border images are not drawn; use a solid, dashed, or dotted border",
	},
	backgroundImage: {
		css: "background-image",
		unsupported: "gradients and background images are not drawn; use background-color",
	},
	borderCollapse: { css: "border-collapse", inherited: true },
	borderSpacing: { css: "border-spacing", inherited: true },
	orphans: { inherited: true },
	widows: { inherited: true },
	objectFit: {
		css: "object-fit",
		unsupported: "size an image with width and height; it always scales to fill them",
	},
	listStyleImage: {
		css: "list-style-image",
		unsupported: "markers are text or drawn shapes; choose list-style-type",
	},
	boxDecorationBreak: {
		css: "box-decoration-break",
		unsupported:
			"a split box draws its top edge on the first page and its bottom edge on the last, as slice does; clone is not implemented",
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
