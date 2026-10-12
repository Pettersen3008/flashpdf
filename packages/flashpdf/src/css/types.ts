import type { Style } from "../element.js";
import type { CssProperty, RejectedProperty } from "./properties.js";

/** An element as selectors see it: its identity and place among the element siblings, with text nodes not counted. */
export type Elem = {
	tag: string;
	id?: string | undefined;
	classes: readonly string[];
	parent: Elem | undefined;
	/** The element sibling just before this one. */
	previous: Elem | undefined;
	/** 1-based place among the element siblings, and how many there are. */
	index: number;
	count: number;
	/** The same among the siblings with this tag. */
	typeIndex: number;
	typeCount: number;
	/** No children but zero-length text. */
	empty: boolean;
	/** Verdicts the matcher caches per compound selector, which keeps combinators polynomial. */
	memo?: Map<object, boolean>;
	before?: Map<object, boolean>;
};
export type Pseudo =
	| { kind: "root" | "empty" }
	| { kind: "nth"; a: number; b: number; last: boolean; type: boolean }
	| { kind: "is" | "where" | "not"; list: readonly (readonly SelectorPiece[])[] };
export type SelectorPart = {
	tag: string;
	ids: readonly string[];
	classes: readonly string[];
	pseudos: readonly Pseudo[];
};
export type SelectorPiece = SelectorPart | " " | ">" | "+" | "~";
export type Specificity = readonly [ids: number, classes: number, tags: number];
export type CascadedStyle = Partial<
	Record<keyof Style | RejectedProperty | CssProperty | `--${string}`, string | number | undefined>
>;
/** An `@property` rule: whether the value inherits, and the value of an element that sets none. */
export type Registration = { inherits: boolean; initial?: string | undefined };
export type Declarations = { normal: CascadedStyle; important: CascadedStyle };
export type Rule = {
	selector: readonly SelectorPiece[];
	order: number;
	specificity: Specificity;
	/** ` in selector ".a" at 1:1`, appended to errors about this rule. */
	where: string;
	/** Set for a rule in an `@media` query outside the subset: `declarations()` throws it, so `render` reports it where the rule matches. */
	blocked?: string | undefined;
	/** Parsed on first use so rules that never match cost nothing and cannot fail a render. */
	declarations(): Declarations;
};
