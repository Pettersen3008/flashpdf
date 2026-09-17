import { number } from "../assert.js";

export const SIDES = ["Top", "Right", "Bottom", "Left"] as const;

export function edgeValues(value: unknown, name: "margin" | "padding"): unknown[] {
	const values = typeof value === "string" ? value.trim().split(/\s+/) : [value];
	if (values.length < 1 || values.length > 4) throw new Error(`invalid ${name}`);
	return values.length === 1
		? [values[0], values[0], values[0], values[0]]
		: values.length === 2
			? [values[0], values[1], values[0], values[1]]
			: values.length === 3
				? [values[0], values[1], values[2], values[1]]
				: values;
}

export function point(value: unknown, parent = 12, positive = false): number {
	if (typeof value === "number") return number(value, positive);
	if (value === "0") return 0;
	if (typeof value !== "string") throw new Error("invalid length");
	const match = /^(\d+(?:\.\d*)?|\.\d+)(pt|px|em|rem)$/.exec(value.trim());
	if (!match) throw new Error("invalid length");
	const scale = match[2] === "px" ? 0.75 : match[2] === "em" ? parent : match[2] === "rem" ? 12 : 1;
	return number(Number(match[1]) * scale, positive);
}
