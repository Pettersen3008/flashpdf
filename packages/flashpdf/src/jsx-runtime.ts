import type { BlockProps, Element, HrProps, PdfProps, TextProps } from "./element.js";

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
		main: BlockProps;
		div: BlockProps;
		section: BlockProps;
		article: BlockProps;
		header: BlockProps;
		footer: BlockProps;
		p: TextProps;
		span: TextProps;
		h1: TextProps;
		h2: TextProps;
		h3: TextProps;
		h4: TextProps;
		h5: TextProps;
		h6: TextProps;
		hr: HrProps;
	}
}
