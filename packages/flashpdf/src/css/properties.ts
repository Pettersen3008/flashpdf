import type { Style } from "../element.js";

export type RejectedProperty =
	| "height"
	| "minWidth"
	| "maxWidth"
	| "borderRadius"
	| "lineHeight"
	| "letterSpacing"
	| "textDecoration"
	| "verticalAlign"
	| "borderCollapse"
	| "borderSpacing";
type StyleProperty = Exclude<keyof Style, `--${string}`>;
type PropertyDefinition = { css?: string; inherited?: boolean; unsupported?: string };
const properties = {
	display: {},
	flexDirection: { css: "flex-direction" },
	flex: {},
	width: {},
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
	background: {},
	backgroundColor: { css: "background-color" },
	gap: {},
	fontSize: { css: "font-size", inherited: true },
	fontFamily: { css: "font-family", inherited: true },
	fontWeight: { css: "font-weight", inherited: true },
	textAlign: { css: "text-align", inherited: true },
	color: { inherited: true },
	breakBefore: { css: "break-before" },
	breakAfter: { css: "break-after" },
	breakInside: { css: "break-inside" },
	height: { unsupported: "block height comes from content" },
	minWidth: {
		css: "min-width",
		unsupported: "only width on a flex row child is implemented",
	},
	maxWidth: {
		css: "max-width",
		unsupported: "only width on a flex row child is implemented",
	},
	borderRadius: { css: "border-radius", unsupported: "box corners are square" },
	lineHeight: {
		css: "line-height",
		unsupported: "line height is fixed to the font's ascent, descent, and line gap",
	},
	letterSpacing: {
		css: "letter-spacing",
		unsupported: "text shaping is not implemented",
	},
	textDecoration: {
		css: "text-decoration",
		unsupported: "text decoration is not implemented",
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
} as const satisfies Record<StyleProperty | RejectedProperty, PropertyDefinition>;

type PropertyName = keyof typeof properties;
const propertyEntries = Object.entries(properties) as [PropertyName, PropertyDefinition][];
const cssNames = Object.fromEntries(
	propertyEntries.map(([name, definition]) => [definition.css ?? name, name]),
) as Record<string, PropertyName>;
export const inherited = new Set<string>(
	propertyEntries.filter(([, definition]) => definition.inherited).map(([name]) => name),
);
export const supported = new Set<keyof Style>(
	propertyEntries
		.filter(([, definition]) => !definition.unsupported)
		.map(([name]) => name) as (keyof Style)[],
);
export const unsupported = Object.fromEntries(
	propertyEntries
		.filter(([, definition]) => definition.unsupported)
		.map(([name, definition]) => [name, definition.unsupported]),
) as Partial<Record<keyof Style | RejectedProperty, string>>;

export function property(
	name: string,
	context = "",
): keyof Style | RejectedProperty | `--${string}` {
	if (name.startsWith("--")) return name as `--${string}`;
	const result = cssNames[name] ?? (name as keyof Style | RejectedProperty);
	const reason = unsupported[result];
	if (reason) throw new Error(`unsupported CSS property: ${name} (${reason})${context}`);
	if (!supported.has(result as never))
		throw new Error(`unsupported CSS property: ${name}${context}`);
	return result;
}
