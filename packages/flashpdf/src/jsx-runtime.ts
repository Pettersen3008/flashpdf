import type { BlockProps, Element, ImageProps, LinkProps, PdfProps, VoidProps } from "./element.js";
import type { BlockTag, TableTag, TextTag } from "./tree.js";

export const Fragment = Symbol.for("flashpdf.fragment");

export function jsx(type: unknown, props: PdfProps, key?: string): Element {
	void key;
	return { type, props };
}

export const jsxs = jsx;
export const jsxDEV = jsx;

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
