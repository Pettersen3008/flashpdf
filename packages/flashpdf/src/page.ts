import { props } from "./assert.js";
import type { Element } from "./element.js";

export const PAGE_NUMBER = "\u001e";
export const TOTAL_PAGES = "\u001f";

/** The only way to emit a page token: the boundary rejects the raw characters in user text. */
export class PageToken {
	constructor(readonly token: string) {}
}

export function PageNumber(): Element {
	return { type: "span", props: { children: new PageToken(PAGE_NUMBER) } };
}

export function TotalPages(): Element {
	return { type: "span", props: { children: new PageToken(TOTAL_PAGES) } };
}

const SIDES = ["top", "right", "bottom", "left"] as const;

type Option = string | number | undefined;
/** The page render options as the core reads them: a size name or width and height, and
 *  shorthand margins or each side, top first. */
export type PageOptions = {
	format: Option | [Option, Option];
	orientation: Option;
	margin: Option | [Option, Option, Option, Option];
};

/** Checks the shape of the page options; the core folds them into `@page` and reports values. */
export function pageOptions(format: unknown, orientation: unknown, margin: unknown): PageOptions {
	const sides =
		typeof margin === "object" && margin !== null && !Array.isArray(margin)
			? props(margin, [...SIDES])
			: undefined;
	const size =
		typeof format === "object" && format !== null && !Array.isArray(format)
			? props(format, ["width", "height"])
			: undefined;
	return {
		format: size
			? [option(size.width, "invalid page width"), option(size.height, "invalid page height")]
			: option(
					format,
					"unsupported page size",
					" (use A3, A4, A5, Letter, Legal, or a width and height)",
				),
		orientation: option(orientation, "invalid orientation"),
		margin: sides
			? [
					option(sides.top, "invalid margin top"),
					option(sides.right, "invalid margin right"),
					option(sides.bottom, "invalid margin bottom"),
					option(sides.left, "invalid margin left"),
				]
			: option(margin, "invalid margin"),
	};
}

function option(value: unknown, message: string, hint = ""): Option {
	if (value === undefined || typeof value === "string" || typeof value === "number") return value;
	throw new Error(`${message}: ${String(value)}${hint}`);
}
