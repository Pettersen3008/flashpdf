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
};

/** Width-like sizes: points (kind 1) or percent of the container's width (kind 2). Heights
 *  are points, percents already resolved against the parent's fixed height. */
export type Size = {
	borderBox: boolean;
	/** One bit per `auto` margin: top 1, right 2, bottom 4, left 8. */
	autoMargin: number;
	width: Dimension | undefined;
	minWidth: Dimension | undefined;
	maxWidth: Dimension | undefined;
	height: number | undefined;
	minHeight: number | undefined;
	maxHeight: number | undefined;
};
type Dimension = { kind: 1 | 2; value: number };

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
	size?: Size | undefined;
};

export type FontSlots = { regular: number; bold?: number | undefined };
