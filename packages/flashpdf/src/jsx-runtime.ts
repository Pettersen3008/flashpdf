import type { BlockProps, Element, ImageProps, LinkProps, PdfProps, VoidProps } from "./element.js";
import type { BlockTag, TableTag, TextTag } from "./tree.js";

export const Fragment = Symbol.for("flashpdf.fragment");

export function jsx(type: unknown, props: PdfProps, key?: string): Element {
	void key;
	return { type, props };
}

export const jsxs = jsx;
export const jsxDEV = jsx;

/** The automatic runtime imports this from the package root when a spread precedes `key`. */
export function createElement(
	type: unknown,
	config: Record<string, unknown> | null | undefined,
	...children: unknown[]
): Element {
	const { key, ...props } = config ?? {};
	void key;
	if (children.length) props.children = children.length === 1 ? children[0] : children;
	return { type, props };
}

export namespace JSX {
	export type Element = import("./element.js").Element;
	export type ElementType = string | ((props: never) => unknown);
	export interface ElementChildrenAttribute {
		children: unknown;
	}
	export interface IntrinsicAttributes {
		key?: string | number;
	}
	export interface IntrinsicElements extends Record<BlockTag | TextTag | TableTag, BlockProps> {
		hr: VoidProps;
		br: VoidProps;
		img: ImageProps;
		a: LinkProps;
	}
}
