import type { Length, StyleInput } from "../element.js";

export type NormalizedStyle = StyleInput & {
	borderTopWidth?: Length;
	borderRightWidth?: Length;
	borderBottomWidth?: Length;
	borderLeftWidth?: Length;
	borderTopColor?: string;
	borderRightColor?: string;
	borderBottomColor?: string;
	borderLeftColor?: string;
	/** Absent means solid, as for a border written with only a width. */
	borderTopStyle?: "solid" | "none";
	borderRightStyle?: "solid" | "none";
	borderBottomStyle?: "solid" | "none";
	borderLeftStyle?: "solid" | "none";
};

export type Box = {
	margin: [number, number, number, number];
	padding: [number, number, number, number];
	border: [number, number, number, number];
	background?: [number, number, number] | undefined;
	borderColor: [
		[number, number, number],
		[number, number, number],
		[number, number, number],
		[number, number, number],
	];
	/** Per side, top first: 0 solid, 1 dashed, 2 dotted. */
	borderStyle: [number, number, number, number];
	radius?: Radius | undefined;
	/** 0 to 1, with every ancestor's opacity multiplied in. */
	opacity: number;
};

/** Eight values, clockwise from the top left corner, horizontal then vertical, in points; bit `i`
 *  of `percent` makes value `i` a percent of the box's width (even `i`) or height (odd `i`). */
export type Radius = { values: number[]; percent: number };

export type FontSlots = {
	regular: number;
	bold?: number | undefined;
	italic?: number | undefined;
	boldItalic?: number | undefined;
};
