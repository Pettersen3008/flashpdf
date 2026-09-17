import { point } from "./length.js";

const BORDER_STYLE = /^(?:none|hidden|dotted|dashed|solid|double|groove|ridge|inset|outset)$/;

/** Only the width is matched by shape; whatever is left over is handed to
 *  `color`, so an unpaintable colour reports itself instead of being ignored. */
export function parseBorder(value: unknown) {
	if (typeof value === "number") return { width: value, color: "black" };
	if (typeof value !== "string") throw new Error("invalid border");
	let width: unknown;
	let paint: string | undefined;
	for (const token of value.trim().split(/\s+/)) {
		if (token === "none" || token === "hidden") return { width: 0, color: "black" };
		if (BORDER_STYLE.test(token)) {
			if (token !== "solid") throw new Error(`unsupported border style: ${token}`);
		} else if (width === undefined && /^[.\d]/.test(token)) width = token;
		else if (paint === undefined) paint = token;
		else throw new Error("invalid border");
	}
	if (width === undefined) throw new Error("invalid border");
	point(width);
	return { width, color: paint ?? "black" };
}
