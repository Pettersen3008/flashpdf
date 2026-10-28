import type {
	BlockProps,
	Element,
	ImageProps,
	LinkProps,
	PdfNode,
	PdfProps,
	SvgProps,
	SvgShapeProps,
	HeadingProps,
	ListItemProps,
	OrderedListProps,
	CellProps,
	VoidProps,
} from "./element.js";
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
		svg: SvgProps;
		g: Omit<SvgShapeProps, "children"> & { children?: PdfNode | undefined };
		rect: SvgShapeProps<"x" | "y" | "width" | "height" | "rx" | "ry">;
		circle: SvgShapeProps<"cx" | "cy" | "r">;
		ellipse: SvgShapeProps<"cx" | "cy" | "rx" | "ry">;
		line: SvgShapeProps<"x1" | "y1" | "x2" | "y2">;
		polyline: SvgShapeProps & { points: string };
		polygon: SvgShapeProps & { points: string };
		path: SvgShapeProps & { d: string };
		title: { children?: string | undefined };
		desc: { children?: string | undefined };
		h1: HeadingProps;
		h2: HeadingProps;
		h3: HeadingProps;
		h4: HeadingProps;
		h5: HeadingProps;
		h6: HeadingProps;
		ol: OrderedListProps;
		li: ListItemProps;
		td: CellProps;
		th: CellProps;
	}
}

/** A host element as plain data, such as parsed JSON: the `{ type, props }` object `jsx()` returns. */
export type ElementData = {
	[Tag in keyof JSX.IntrinsicElements]: { type: Tag; props: JSX.IntrinsicElements[Tag] };
}[keyof JSX.IntrinsicElements];
