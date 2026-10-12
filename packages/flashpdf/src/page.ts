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
