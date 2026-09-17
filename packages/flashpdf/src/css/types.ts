import type { Style } from "../element.js";
import type { RejectedProperty } from "./properties.js";

export type Identity = { tag: string; id?: string | undefined; classes: readonly string[] };
export type SelectorPart = { tag: string; ids: readonly string[]; classes: readonly string[] };
export type SelectorPiece = SelectorPart | " " | ">";
export type CascadedStyle = Partial<
	Record<keyof Style | RejectedProperty | `--${string}`, string | number | undefined>
>;
export type Rule = {
	selector: readonly SelectorPiece[];
	declarations: CascadedStyle;
	order: number;
	specificity: number;
};
