import type { Style } from "../element.js";
import { point } from "../style/length.js";
import { tags } from "../tree.js";
import { property } from "./properties.js";
import type {
	Declarations,
	Identity,
	Registration,
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

/** End of the escape at the backslash `index`: up to six hex digits and one whitespace, or one
 *  other character. -1 for a backslash that ends the source or precedes a newline. */
function escapeEnd(source: string, index: number) {
	const next = source[index + 1];
	if (next === undefined || next === "\n") return -1;
	let end = index + 2;
	if (/[\da-f]/i.test(next)) {
		while (end < index + 7 && /[\da-f]/i.test(source[end] ?? "")) end++;
		if (whitespace(source[end] ?? "")) end++;
	}
	return end;
}
/** End of the name at `start`: letters, digits, `_`, `-`, and escapes. A scanner, not a regex,
 *  because a hex escape and the name characters after it overlap and would backtrack. */
function nameEnd(source: string, start: number) {
	let index = start;
	while (index < source.length) {
		if (source[index] === "\\") {
			const end = escapeEnd(source, index);
			if (end < 0) break;
			index = end;
		} else if (/[\w-]/.test(source[index]!)) index++;
		else break;
	}
	return index;
}
/** CSS escapes: `\/` is `/`, `\32 ` is `2`; an invalid code point is U+FFFD. */
function unescape(name: string) {
	return name.replace(
		/\\(?:([\da-f]{1,6})[ \t\n\r\f]?|([^\n]))/gi,
		(_, hex: string | undefined, other: string | undefined) => {
			if (other !== undefined) return other;
			const code = Number.parseInt(hex!, 16);
			return code === 0 || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)
				? "\ufffd"
				: String.fromCodePoint(code);
		},
	);
}
function skipSpace(source: string, index: number) {
	while (whitespace(source[index] ?? "")) index++;
	return index;
}
/** A type, then `#id` and `.class` names, ending at the first other character. Undefined if empty or escaped in the type. */
function compound(source: string, start: number): [SelectorPart, number] | undefined {
	const ids: string[] = [];
	const classes: string[] = [];
	let index = start;
	if (/[a-z]/i.test(source[index] ?? "")) index = nameEnd(source, index);
	// HTML type selectors are case-insensitive; JSX tags are lowercase.
	const tag = source.slice(start, index).toLowerCase();
	if (tag.includes("\\")) return undefined;
	while (source[index] === "#" || source[index] === ".") {
		const end = nameEnd(source, index + 1);
		if (end === index + 1) return undefined;
		(source[index] === "#" ? ids : classes).push(unescape(source.slice(index + 1, end)));
		index = end;
	}
	return index === start ? undefined : [{ tag, ids, classes }, index];
}
/** Returns undefined for selector syntax outside the subset (`*`, pseudo-classes and
 *  pseudo-elements, attributes, sibling combinators) so the caller can skip the rule. A
 *  type selector FlashPDF never renders (`html`, `body`, `button`) can never match, so it
 *  is skipped too. */
export function selector(source: string): SelectorPiece[] | undefined {
	const compiled: SelectorPiece[] = [];
	let index = 0;
	while (true) {
		const found = compound(source, index);
		if (!found) return undefined;
		compiled.push(found[0]);
		const after = skipSpace(source, found[1]);
		if (after === source.length) break;
		if (source[after] === ">") {
			index = skipSpace(source, after + 1);
			compiled.push(">");
		} else if (after > found[1]) {
			index = after;
			compiled.push(" ");
		} else return undefined;
	}
	return compiled.some(
		(part) =>
			typeof part !== "string" && part.tag && !(tags as readonly string[]).includes(part.tag),
	)
		? undefined
		: compiled;
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
 *  ancestors, so `div > section p` matches a `p` inside nested sections.
 *  `failed` remembers each (piece, depth) whose ancestor search found nothing,
 *  which keeps the search polynomial instead of exponential. */
function matchesFrom(
	pieces: readonly SelectorPiece[],
	index: number,
	node: Identity | undefined,
	ancestors: readonly Identity[],
	depth: number,
	failed: Set<number>,
): boolean {
	if (!matchesPart(pieces[index], node)) return false;
	if (index === 0) return true;
	if (pieces[index - 1] === ">")
		return matchesFrom(pieces, index - 2, ancestors[depth - 1], ancestors, depth - 1, failed);
	const key = index * (ancestors.length + 1) + depth;
	if (failed.has(key)) return false;
	for (let ancestor = depth - 1; ancestor >= 0; ancestor--)
		if (matchesFrom(pieces, index - 2, ancestors[ancestor], ancestors, ancestor, failed))
			return true;
	failed.add(key);
	return false;
}
export function matches(
	pieces: readonly SelectorPiece[],
	node: Identity,
	ancestors: readonly Identity[],
) {
	return matchesFrom(pieces, pieces.length - 1, node, ancestors, ancestors.length, new Set());
}

function position(source: string, index: number) {
	const before = source.slice(0, index);
	return `${before.split("\n").length}:${before.length - before.lastIndexOf("\n")}`;
}
function invalid(source: string, index: number) {
	return new Error(`invalid CSS stylesheet at ${position(source, index)}`);
}
/** Blanks comments to same-length whitespace, keeping newlines, so error positions match the source. */
function withoutComments(source: string) {
	const parts: string[] = [];
	let index = 0;
	while (true) {
		const start = source.indexOf("/*", index);
		if (start < 0) return parts.join("") + source.slice(index);
		parts.push(source.slice(index, start));
		const end = source.indexOf("*/", start + 2);
		if (end < 0) return parts.join("") + source.slice(start);
		parts.push(source.slice(start, end + 2).replace(/[^\n]/g, " "));
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
/** Index of the quote closing the string that opens at `start`. */
function quoted(source: string, start: number) {
	for (let index = start + 1; index < source.length; index++) {
		if (source[index] === "\\") index++;
		else if (source[index] === source[start]) return index;
	}
	return source.length;
}
/** Index just past the `}` matching the `{` at `open`; braces inside strings do not count,
 *  and an escape such as the `\'` in Tailwind's `.\[\&_svg\:not\(\[class\*\=\'size-\'\]\)\]`
 *  opens no string. */
function closing(source: string, open: number) {
	let depth = 0;
	for (let index = open; index < source.length; index++) {
		const character = source[index];
		if (character === "\\") index++;
		else if (character === '"' || character === "'") index = quoted(source, index);
		else if (character === "{") depth++;
		else if (character === "}" && --depth === 0) return index + 1;
	}
	throw invalid(source, open);
}
/** Index of the `{` or `;` that ends the at-rule prelude starting at `start`. */
function preludeEnd(source: string, start: number) {
	for (let index = start; index < source.length; index++) {
		const character = source[index];
		if (character === "\\") index++;
		else if (character === '"' || character === "'") index = quoted(source, index);
		else if (character === ";" || character === "{") return index;
		else if (character === "}") break;
	}
	throw invalid(source, start);
}

const COMPARE = {
	">=": (width: number, limit: number) => width >= limit,
	"<=": (width: number, limit: number) => width <= limit,
	">": (width: number, limit: number) => width > limit,
	"<": (width: number, limit: number) => width < limit,
	"=": (width: number, limit: number) => width === limit,
};
const WIDTH = /^\(\s*(?:(min|max)-width\s*:|width\s*(?::|(>=|<=|>|<|=)))\s*([^()]+?)\s*\)$/;
/** Whether an `@media` prelude matches a page `width` points wide: `print` and `all` match, `screen`
 *  does not, and a width condition compares the width. With no `width`, a width condition counts as
 *  matching, so `stylesheet()` validates the rules it guards. Throws for anything else. */
function media(prelude: string, width: number | undefined): boolean {
	return prelude
		.toLowerCase()
		.split(",")
		.some((alternative) => {
			const query = alternative.trim();
			const modifier = /^(not|only)\s+/.exec(query);
			const matched = query
				.slice(modifier?.[0].length ?? 0)
				.split(/\s+and\s+/)
				.every((term) => {
					if (term === "all" || term === "print") return true;
					if (term === "screen") return false;
					const feature = WIDTH.exec(term);
					if (!feature)
						throw new Error("only all, print, screen, and width conditions are supported");
					const operator =
						feature[2] ?? (feature[1] === "min" ? ">=" : feature[1] === "max" ? "<=" : "=");
					const limit = point(feature[3], 12);
					return width === undefined || COMPARE[operator as keyof typeof COMPARE](width, limit);
				});
			return modifier?.[1] === "not" ? !matched : matched;
		});
}

function register(name: string, body: string): Registration {
	const descriptors = new Map<string, string>();
	for (const item of body.split(";")) {
		const colon = item.indexOf(":");
		if (colon > 0)
			descriptors.set(item.slice(0, colon).trim().toLowerCase(), item.slice(colon + 1).trim());
	}
	const inherits = descriptors.get("inherits");
	if (
		!/^--[\w-]+$/.test(name) ||
		!descriptors.has("syntax") ||
		(inherits !== "true" && inherits !== "false")
	)
		throw new Error(`invalid @property ${name}: it needs syntax and inherits`);
	return { inherits: inherits === "true", initial: descriptors.get("initial-value") || undefined };
}

export type ParseContext = {
	/** The page content width in points that `@media` width conditions compare. */
	width?: number | undefined;
	/** Collects the stylesheet's `@property` rules by name. */
	properties?: Map<string, Registration> | undefined;
};
const MAX_MEDIA_DEPTH = 16;

/** Parses the rules in `clean[start, stop)`. `@media` blocks recurse and `@property` registers;
 *  every other at-rule (`@layer`, `@supports`, `@import`, ...) is skipped whole. */
function parseBlock(
	clean: string,
	start: number,
	stop: number,
	rules: Rule[],
	context: ParseContext,
	depth: number,
	blocked?: string,
) {
	let index = start;
	while (index < stop) {
		if (whitespace(clean[index]!)) {
			index++;
			continue;
		}
		if (clean[index] === "@") {
			const name = /^@([\w-]+)/.exec(clean.slice(index, index + 32))?.[1]?.toLowerCase();
			const open = preludeEnd(clean, index);
			const end = clean[open] === ";" ? open + 1 : closing(clean, open);
			if (clean[open] === "{" && name === "media") {
				if (depth >= MAX_MEDIA_DEPTH)
					throw new Error(
						`@media nests deeper than ${MAX_MEDIA_DEPTH} levels at ${position(clean, index)}`,
					);
				const prelude = clean.slice(index + 6, open).trim();
				let reason = blocked;
				let applies = true;
				try {
					applies = media(prelude, context.width);
				} catch (error) {
					reason ??= `unsupported media query "${prelude}" (${(error as Error).message})`;
				}
				if (applies) parseBlock(clean, open + 1, end - 1, rules, context, depth + 1, reason);
			} else if (clean[open] === "{" && name === "property" && depth === 0) {
				const custom = clean.slice(index + 9, open).trim();
				const registration = register(custom, clean.slice(open + 1, end - 1));
				context.properties?.set(custom, registration);
			}
			index = end;
			continue;
		}
		const open = clean.indexOf("{", index);
		if (open <= index || open >= stop || clean.slice(index, open).includes("}"))
			throw invalid(clean, index);
		const end = closing(clean, open);
		const body = clean.slice(open + 1, end - 1);
		const at = ` at ${position(clean, index)}`;
		// An unescaped comma separates selectors; `\,` belongs to a class name.
		const selectors = clean.slice(index, open).match(/(?:\\[\s\S]|[^,\\])+/g) ?? [];
		index = end;
		let cached: Declarations | undefined;
		for (const raw of selectors) {
			const text = raw.trim();
			const compiled = selector(text);
			if (!compiled) continue;
			const where = ` in selector ${JSON.stringify(text)}${at}`;
			rules.push({
				selector: compiled,
				order: rules.length,
				specificity: specificity(compiled),
				where,
				blocked,
				declarations:
					blocked === undefined
						? () => (cached ??= declarations(body, where))
						: () => {
								throw new Error(`${blocked}${where}`);
							},
			});
		}
	}
}

export function parse(source: string, context: ParseContext = {}): Rule[] {
	const rules: Rule[] = [];
	const clean = withoutComments(source);
	parseBlock(clean, 0, clean.length, rules, context, 0);
	return rules;
}

export function inline(value: Style | string | undefined): Declarations {
	if (value === undefined) return { normal: {}, important: {} };
	return typeof value === "string"
		? declarations(value, " in inline style")
		: { normal: value, important: {} };
}
