import type { Style } from "../element.js";
import type { RejectedProperty } from "./properties.js";

export type Identity = { tag: string; id?: string | undefined; classes: readonly string[] };
export type SelectorPart = { tag: string; ids: readonly string[]; classes: readonly string[] };
export type SelectorPiece = SelectorPart | " " | ">";
export type Specificity = readonly [ids: number, classes: number, tags: number];
export type CascadedStyle = Partial<
	Record<keyof Style | RejectedProperty | `--${string}`, string | number | undefined>
>;
export type Declarations = { normal: CascadedStyle; important: CascadedStyle };
export type Rule = {
	selector: readonly SelectorPiece[];
	order: number;
	specificity: Specificity;
	/** Parsed on first use so rules that never match cost nothing and cannot fail a render. */
	declarations(): Declarations;
};
