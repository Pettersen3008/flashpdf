import type { BlockProps, Element, HrProps, PdfProps, TextProps } from "./element.js";
import type { BlockTag, TextTag } from "./tree.js";

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
	export interface IntrinsicElements
		extends Record<BlockTag, BlockProps>, Record<TextTag, TextProps> {
		hr: HrProps;
	}
}
