import type { Style } from "../element.js";
import { property } from "./properties.js";
import type {
	Declarations,
	Identity,
	Rule,
	SelectorPart,
	SelectorPiece,
	Specificity,
} from "./types.js";

const IMPORTANT = /\s*!\s*important$/i;

function declarations(source: string, context = ""): Declarations {
	const result: Declarations = { normal: {}, important: {} };
	for (const item of source.split(";")) {
		if (!item.trim()) continue;
		const colon = item.indexOf(":");
		if (colon < 1) throw new Error(`invalid CSS declaration: ${item.trim()}${context}`);
		const key = property(item.slice(0, colon).trim(), context);
		const value = item.slice(colon + 1).trim();
		const target = (IMPORTANT.test(value) ? result.important : result.normal) as Record<
			string,
			string
		>;
		delete target[key];
		target[key] = value.replace(IMPORTANT, "");
	}
	return result;
}

const PART = /^(?:[a-z][\w-]*)?(?:#[\w-]+|\.[\w-]+)*$/i;
function selectorPart(part: string): SelectorPart {
	return {
		tag: part.match(/^[a-z][\w-]*/i)?.[0] ?? "",
		ids: part.match(/#[\w-]+/g)?.map((value) => value.slice(1)) ?? [],
		classes: part.match(/\.[\w-]+/g)?.map((value) => value.slice(1)) ?? [],
	};
}
/** Returns undefined for selector syntax outside the subset (`*`, pseudo-classes,
 *  attributes, escapes, sibling combinators) so the caller can skip the rule. */
export function selector(source: string): SelectorPiece[] | undefined {
	const pieces = normalizeSelector(source).split(/(>| )/).filter(Boolean);
	if (
		pieces.length % 2 === 0 ||
		pieces.some((part, index) => (index % 2 ? part !== " " && part !== ">" : !PART.test(part)))
	)
		return undefined;
	return pieces.map((part, index) => (index % 2 === 0 ? selectorPart(part) : (part as " " | ">")));
}
function specificity(pieces: readonly SelectorPiece[]): Specificity {
	let ids = 0;
	let classes = 0;
	let tags = 0;
	for (const part of pieces)
		if (typeof part !== "string") {
			ids += part.ids.length;
			classes += part.classes.length;
			tags += Number(Boolean(part.tag));
		}
	return [ids, classes, tags];
}
export function compareSpecificity(a: Specificity, b: Specificity): number {
	return a[0] - b[0] || a[1] - b[1] || a[2] - b[2];
}
function matchesPart(part: SelectorPiece | undefined, node: Identity | undefined) {
	if (part === undefined || typeof part === "string" || node === undefined) return false;
	return (
		(!part.tag || part.tag === node.tag) &&
		part.ids.every((id) => id === node.id) &&
		part.classes.every((value) => node.classes.includes(value))
	);
}
/** Right-to-left with backtracking: a descendant step may skip any number of
 *  ancestors, so `div > section p` matches a `p` inside nested sections. */
function matchesFrom(
	pieces: readonly SelectorPiece[],
	index: number,
	node: Identity | undefined,
	ancestors: readonly Identity[],
	depth: number,
): boolean {
	if (!matchesPart(pieces[index], node)) return false;
	if (index === 0) return true;
	if (pieces[index - 1] === ">")
		return matchesFrom(pieces, index - 2, ancestors[depth - 1], ancestors, depth - 1);
	for (let ancestor = depth - 1; ancestor >= 0; ancestor--)
		if (matchesFrom(pieces, index - 2, ancestors[ancestor], ancestors, ancestor)) return true;
	return false;
}
export function matches(
	pieces: readonly SelectorPiece[],
	node: Identity,
	ancestors: readonly Identity[],
) {
	return matchesFrom(pieces, pieces.length - 1, node, ancestors, ancestors.length);
}

function position(source: string, index: number) {
	const before = source.slice(0, index);
	return `${before.split("\n").length}:${before.length - before.lastIndexOf("\n")}`;
}
function invalid(source: string, index: number) {
	return new Error(`invalid CSS stylesheet at ${position(source, index)}`);
}
function withoutComments(source: string) {
	const parts: string[] = [];
	let index = 0;
	while (true) {
		const start = source.indexOf("/*", index);
		if (start < 0) return parts.join("") + source.slice(index);
		parts.push(source.slice(index, start));
		const end = source.indexOf("*/", start + 2);
		if (end < 0) return parts.join("") + source.slice(start);
		index = end + 2;
	}
}
function whitespace(character: string) {
	return (
		character === " " ||
		character === "\t" ||
		character === "\n" ||
		character === "\r" ||
		character === "\f"
	);
}
function normalizeSelector(selector: string) {
	let result = "";
	let pendingSpace = false;
	for (const character of selector) {
		if (whitespace(character)) {
			pendingSpace = result.length > 0;
		} else if (character === ">") {
			result = result.trimEnd() + character;
			pendingSpace = false;
		} else {
			if (pendingSpace && !result.endsWith(">")) result += " ";
			result += character;
			pendingSpace = false;
		}
	}
	return result;
}
/** Index of the quote closing the string that opens at `start`. */
function quoted(source: string, start: number) {
	for (let index = start + 1; index < source.length; index++) {
		if (source[index] === "\\") index++;
		else if (source[index] === source[start]) return index;
	}
	return source.length;
}
/** Index just past the `}` matching the `{` at `open`; braces inside strings do not count. */
function closing(source: string, open: number) {
	let depth = 0;
	for (let index = open; index < source.length; index++) {
		const character = source[index];
		if (character === '"' || character === "'") index = quoted(source, index);
		else if (character === "{") depth++;
		else if (character === "}" && --depth === 0) return index + 1;
	}
	throw invalid(source, open);
}
/** At-rules (`@layer`, `@media`, `@property`, `@import`, ...) are skipped whole. */
function skipAtRule(source: string, start: number) {
	for (let index = start; index < source.length; index++) {
		const character = source[index];
		if (character === '"' || character === "'") index = quoted(source, index);
		else if (character === ";") return index + 1;
		else if (character === "{") return closing(source, index);
		else if (character === "}") break;
	}
	throw invalid(source, start);
}
export function parse(source: string): Rule[] {
	const rules: Rule[] = [];
	const clean = withoutComments(source);
	let index = 0;
	while (index < clean.length) {
		if (whitespace(clean[index]!)) {
			index++;
			continue;
		}
		if (clean[index] === "@") {
			index = skipAtRule(clean, index);
			continue;
		}
		const open = clean.indexOf("{", index);
		if (open <= index || clean.slice(index, open).includes("}")) throw invalid(clean, index);
		const end = closing(clean, open);
		const body = clean.slice(open + 1, end - 1);
		const at = ` at ${position(clean, index)}`;
		const selectors = clean.slice(index, open).split(",");
		index = end;
		let cached: Declarations | undefined;
		for (const raw of selectors) {
			const text = raw.trim();
			const compiled = selector(text);
			if (!compiled) continue;
			rules.push({
				selector: compiled,
				order: rules.length,
				specificity: specificity(compiled),
				declarations: () =>
					(cached ??= declarations(body, ` in selector ${JSON.stringify(text)}${at}`)),
			});
		}
	}
	return rules;
}

export function inline(value: Style | string | undefined): Declarations {
	if (value === undefined) return { normal: {}, important: {} };
	return typeof value === "string"
		? declarations(value, " in inline style")
		: { normal: value, important: {} };
}
