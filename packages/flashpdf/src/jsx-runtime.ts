import type { Element, PdfProps } from "./element.js";

export const Fragment = Symbol.for("flashpdf.fragment");

export function jsx(type: unknown, props: PdfProps, key?: string): Element {
	void key;
	return { type, props };
}

export const jsxs = jsx;
export const jsxDEV = jsx;

export namespace JSX {
	export type Element = import("./element.js").Element;
	export type ElementType = string | ((props: any) => any);
	export interface ElementChildrenAttribute {
		children: unknown;
	}
	export interface IntrinsicAttributes {
		key?: string | number;
	}
	export interface IntrinsicElements {
		[element: string]: PdfProps;
	}
}
