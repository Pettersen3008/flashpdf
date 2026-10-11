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
};

export type FontSlots = { regular: number; bold?: number | undefined };
