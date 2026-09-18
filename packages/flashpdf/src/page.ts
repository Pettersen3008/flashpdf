import type { Element } from "./element.js";

export const PAGE_NUMBER = "\u001e";
export const TOTAL_PAGES = "\u001f";

export function PageNumber(): Element {
	return { type: "span", props: { children: PAGE_NUMBER } };
}

export function TotalPages(): Element {
	return { type: "span", props: { children: TOTAL_PAGES } };
}
