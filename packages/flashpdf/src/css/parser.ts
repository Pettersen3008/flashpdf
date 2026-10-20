import type { Style } from "../element.js";
import { point } from "../style/length.js";
import { tags } from "../tree.js";
import { property } from "./properties.js";
import type {
	Declarations,
	Elem,
	Pseudo,
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
const MAX_SELECTOR_DEPTH = 16;
/** Pseudo-classes and CSS 2 pseudo-elements that match nothing in a PDF: no pointer, focus, form, or generated content. */
const NEVER = new Set(
	`before after first-line first-letter hover active focus focus-visible focus-within visited
	target target-within local-link checked indeterminate default enabled disabled required optional
	read-only read-write placeholder-shown valid invalid user-valid user-invalid in-range out-of-range
	autofill fullscreen modal open popover-open picture-in-picture paused playing muted seeking
	buffering stalled volume-locked current past future host host-context -moz-focusring
	-moz-ui-invalid -webkit-autofill -moz-placeholder -ms-input-placeholder`.split(/\s+/),
);
type Parse = {
	where: string;
	/** The first feature outside the subset; it rejects the selector unless the selector can never match. */
	why?: string | undefined;
};

const badSelector = (parse: Parse) => new Error(`invalid CSS selector${parse.where}`);
/** Index of the bracket closing the `(` or `[` at `open`, or -1. */
function bracketEnd(source: string, open: number) {
	let depth = 0;
	for (let index = open; index < source.length; index++) {
		const character = source[index];
		if (character === "\\") index++;
		else if (character === '"' || character === "'") index = quoted(source, index);
		else if (character === "(" || character === "[") depth++;
		else if ((character === ")" || character === "]") && --depth === 0) return index;
	}
	return -1;
}
/** Splits at the commas outside brackets, strings, and escapes: `\,` belongs to a class name. */
function split(source: string) {
	const parts: string[] = [];
	let depth = 0;
	let start = 0;
	for (let index = 0; index < source.length; index++) {
		const character = source[index];
		if (character === "\\") index++;
		else if (character === '"' || character === "'") index = quoted(source, index);
		else if (character === "(" || character === "[") depth++;
		else if (character === ")" || character === "]") depth--;
		else if (character === "," && depth === 0) {
			parts.push(source.slice(start, index));
			start = index + 1;
		}
	}
	return [...parts, source.slice(start)];
}
/** `odd`, `even`, `B`, or `An+B` as [a, b]; undefined for anything else, `of <selector>` included. */
function nth(source: string): [a: number, b: number] | undefined {
	const text = source.trim().toLowerCase();
	if (text === "odd") return [2, 1];
	if (text === "even") return [2, 0];
	const found = /^(?:([+-]?)(\d{0,9})n(?:\s*([+-])\s*(\d{1,9}))?|([+-]?\d{1,9}))$/.exec(text);
	if (!found) return undefined;
	if (found[5] !== undefined) return [0, Number(found[5])];
	return [
		(found[1] === "-" ? -1 : 1) * Number(found[2] || 1),
		found[3] === undefined ? 0 : (found[3] === "-" ? -1 : 1) * Number(found[4]),
	];
}
/** The `:pseudo` at `start` as the pseudo-classes it adds, or undefined when it can match no rendered element. */
function pseudo(
	source: string,
	start: number,
	depth: number,
	parse: Parse,
): [Pseudo[] | undefined, number] {
	const element = source[start + 1] === ":";
	const from = start + (element ? 2 : 1);
	const end = nameEnd(source, from);
	if (end === from) throw badSelector(parse);
	const name = unescape(source.slice(from, end)).toLowerCase();
	let index = end;
	let args: string | undefined;
	if (source[index] === "(") {
		const close = bracketEnd(source, index);
		if (close < 0) throw badSelector(parse);
		args = source.slice(index + 1, close);
		index = close + 1;
	}
	if (element || NEVER.has(name)) return [undefined, index];
	const place = /^(first|last|only)-(child|of-type)$/.exec(name);
	const counted = /^nth-(last-)?(child|of-type)$/.exec(name);
	if (place && args === undefined) {
		const type = place[2] === "of-type";
		const found: Pseudo[] = [];
		if (place[1] !== "last") found.push({ kind: "nth", a: 0, b: 1, last: false, type });
		if (place[1] !== "first") found.push({ kind: "nth", a: 0, b: 1, last: true, type });
		return [found, index];
	}
	if (counted && args !== undefined) {
		const step = nth(args);
		if (step)
			return [
				[
					{
						kind: "nth",
						a: step[0],
						b: step[1],
						last: Boolean(counted[1]),
						type: counted[2] === "of-type",
					},
				],
				index,
			];
		parse.why ??= `:${name}(${args.trim()}) (use An+B, odd, or even; "of <selector>" is not implemented)`;
		return [[], index];
	}
	if ((name === "root" || name === "empty") && args === undefined) return [[{ kind: name }], index];
	if ((name === "is" || name === "where" || name === "not") && args !== undefined) {
		if (depth >= MAX_SELECTOR_DEPTH)
			throw new Error(`:${name}() nests deeper than ${MAX_SELECTOR_DEPTH} levels${parse.where}`);
		const list = alternatives(args, depth + 1, parse);
		if (name === "not") return [list.length ? [{ kind: name, list }] : [], index];
		return [list.length ? [{ kind: name, list }] : undefined, index];
	}
	parse.why ??= `:${name}${args === undefined ? "" : "()"} (not implemented)`;
	return [[], index];
}
/** The arguments that can match something. One that cannot drops out, so `:not(:hover)` is true and `:is(:hover)` is not. */
function alternatives(source: string, depth: number, parse: Parse) {
	const found: SelectorPiece[][] = [];
	if (source.trim())
		for (const item of split(source)) {
			const own: Parse = { where: parse.where };
			const pieces = complex(item.trim(), depth, own);
			if (!pieces) continue;
			found.push(pieces);
			parse.why ??= own.why;
		}
	return found;
}
/** A type or `*`, then `#id`, `.class`, `:pseudo`, and `[attribute]` parts; undefined if no rendered element can match it. */
function compound(
	source: string,
	start: number,
	depth: number,
	parse: Parse,
): [SelectorPart | undefined, number] {
	const ids: string[] = [];
	const classes: string[] = [];
	const pseudos: Pseudo[] = [];
	let index = start;
	let tag = "";
	if (source[index] === "*") index++;
	else if (/[a-z]/i.test(source[index] ?? "")) {
		const end = nameEnd(source, index);
		// HTML type selectors are case-insensitive; JSX tags are lowercase.
		tag = unescape(source.slice(index, end)).toLowerCase();
		index = end;
	}
	let never = tag !== "" && !(tags as readonly string[]).includes(tag);
	if (source[index] === "|") {
		parse.why ??= "namespaces (FlashPDF elements have none)";
		index = source[index + 1] === "*" ? index + 2 : nameEnd(source, index + 1);
	}
	for (let character = source[index]; ; character = source[index]) {
		if (character === "#" || character === ".") {
			const end = nameEnd(source, index + 1);
			if (end === index + 1) throw badSelector(parse);
			(character === "#" ? ids : classes).push(unescape(source.slice(index + 1, end)));
			index = end;
		} else if (character === ":") {
			const [added, end] = pseudo(source, index, depth, parse);
			if (added) pseudos.push(...added);
			else never = true;
			index = end;
		} else if (character === "[") {
			const end = bracketEnd(source, index);
			if (end < 0) throw badSelector(parse);
			parse.why ??= "attribute selectors (match a class or id instead)";
			index = end + 1;
		} else break;
	}
	if (index === start) throw badSelector(parse);
	return [never ? undefined : { tag, ids, classes, pseudos }, index];
}
/** Compounds joined by combinators; undefined when any compound can match no rendered element. */
function complex(source: string, depth: number, parse: Parse): SelectorPiece[] | undefined {
	const pieces: SelectorPiece[] = [];
	let never = false;
	let index = 0;
	while (true) {
		const [part, end] = compound(source, index, depth, parse);
		if (part) pieces.push(part);
		else never = true;
		const after = skipSpace(source, end);
		if (after === source.length) break;
		const combinator = source[after];
		if (combinator === ">" || combinator === "+" || combinator === "~") {
			index = skipSpace(source, after + 1);
			pieces.push(combinator);
		} else if (after > end) {
			index = after;
			pieces.push(" ");
		} else throw badSelector(parse);
	}
	return never ? undefined : pieces;
}
/** Undefined when no rendered element can match, so the rule is skipped; throws for what the subset lacks. */
export function selector(source: string, where: string): SelectorPiece[] | undefined {
	const parse: Parse = { where };
	const pieces = complex(source, 0, parse);
	if (pieces && parse.why) throw new Error(`unsupported CSS selector: ${parse.why}${where}`);
	return pieces;
}
const ZERO: Specificity = [0, 0, 0];
/** `:is()` and `:not()` count their most specific argument, `:where()` nothing, other pseudo-classes a class. */
export function specificity(pieces: readonly SelectorPiece[]): Specificity {
	const total = [0, 0, 0] as [number, number, number];
	for (const part of pieces) {
		if (typeof part === "string") continue;
		const weights: Specificity[] = [
			[part.ids.length, part.classes.length, Number(Boolean(part.tag))],
		];
		for (const pseudo of part.pseudos)
			weights.push(
				pseudo.kind === "where"
					? ZERO
					: "list" in pseudo
						? pseudo.list.map(specificity).reduce((a, b) => (compareSpecificity(a, b) < 0 ? b : a))
						: [0, 1, 0],
			);
		for (const weight of weights)
			for (let place = 0; place < 3; place++) total[place]! += weight[place]!;
	}
	return total;
}
export function compareSpecificity(a: Specificity, b: Specificity): number {
	return a[0] - b[0] || a[1] - b[1] || a[2] - b[2];
}
function matchesPseudo(pseudo: Pseudo, node: Elem): boolean {
	switch (pseudo.kind) {
		case "root":
			return node.parent === undefined;
		case "empty":
			return node.empty;
		case "nth": {
			const [index, count] = pseudo.type
				? [node.typeIndex, node.typeCount]
				: [node.index, node.count];
			const gap = (pseudo.last ? count - index + 1 : index) - pseudo.b;
			return pseudo.a === 0 ? gap === 0 : gap % pseudo.a === 0 && gap / pseudo.a >= 0;
		}
		default:
			return (
				pseudo.list.some((item) => matchesFrom(item, item.length - 1, node)) ===
				(pseudo.kind !== "not")
			);
	}
}
function matchesPart(part: SelectorPart, node: Elem) {
	return (
		(!part.tag || part.tag === node.tag) &&
		part.ids.every((id) => id === node.id) &&
		part.classes.every((value) => node.classes.includes(value)) &&
		part.pseudos.every((pseudo) => matchesPseudo(pseudo, node))
	);
}
/** Whether a sibling before `node` passes `test`; each sibling the walk crosses caches the verdict, so it is linear. */
function earlier(node: Elem, key: object, test: (sibling: Elem) => boolean) {
	const crossed: Elem[] = [];
	let verdict = false;
	for (let at: Elem | undefined = node; at; at = at.previous) {
		const known = at.before?.get(key);
		if (known !== undefined) {
			verdict = known;
			break;
		}
		crossed.push(at);
		if (at.previous && test(at.previous)) {
			verdict = true;
			break;
		}
	}
	for (const sibling of crossed) (sibling.before ??= new Map()).set(key, verdict);
	return verdict;
}
function matchesAt(pieces: readonly SelectorPiece[], index: number, node: Elem): boolean {
	if (!matchesPart(pieces[index] as SelectorPart, node)) return false;
	if (index === 0) return true;
	const left = (other: Elem | undefined) =>
		other !== undefined && matchesFrom(pieces, index - 2, other);
	switch (pieces[index - 1]) {
		case ">":
			return left(node.parent);
		case "+":
			return left(node.previous);
		case "~":
			return earlier(node, pieces[index - 2] as SelectorPart, left);
		default:
			for (let ancestor = node.parent; ancestor; ancestor = ancestor.parent)
				if (left(ancestor)) return true;
			return false;
	}
}
/** Right-to-left, with one cached verdict per (compound, element) so backtracking stays polynomial. */
function matchesFrom(pieces: readonly SelectorPiece[], index: number, node: Elem): boolean {
	// The last compound has one caller per element, so it needs no cache.
	if (index === pieces.length - 1) return matchesAt(pieces, index, node);
	const part = pieces[index]!;
	const known = node.memo?.get(part as object);
	if (known !== undefined) return known;
	const verdict = matchesAt(pieces, index, node);
	(node.memo ??= new Map()).set(part as object, verdict);
	return verdict;
}
export function matches(pieces: readonly SelectorPiece[], node: Elem) {
	return matchesFrom(pieces, pieces.length - 1, node);
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
/** One `@page` declaration, in source order. */
export type PageDeclaration = { name: string; value: string; important: boolean; where: string };
const PAGE_PROPERTIES = [
	"size",
	"margin",
	"margin-top",
	"margin-right",
	"margin-bottom",
	"margin-left",
];
/** Reads an `@page` rule into `pages` and returns the index past it. */
function pageRule(source: string, start: number, pages: PageDeclaration[]) {
	const open = source.indexOf("{", start);
	if (open < 0) throw invalid(source, start);
	const where = ` in @page at ${position(source, start)}`;
	const selector = source.slice(start + 5, open).trim();
	if (selector)
		throw new Error(
			`unsupported @page selector: ${selector} (one size and margin apply to every page)${where}`,
		);
	const end = closing(source, open);
	const body = source.slice(open + 1, end - 1);
	const box = /@[\w-]+/.exec(body);
	if (box)
		throw new Error(
			`unsupported @page margin box: ${box[0]} (use the header and footer options)${where}`,
		);
	for (const item of body.split(";")) {
		if (!item.trim()) continue;
		const colon = item.indexOf(":");
		if (colon < 1) throw new Error(`invalid CSS declaration: ${item.trim()}${where}`);
		const name = item.slice(0, colon).trim();
		if (!PAGE_PROPERTIES.includes(name))
			throw new Error(`unsupported @page property: ${name} (only size and margin apply)${where}`);
		const value = item.slice(colon + 1).trim();
		pages.push({
			name,
			value: value.replace(IMPORTANT, ""),
			important: IMPORTANT.test(value),
			where,
		});
	}
	return end;
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
	pages?: PageDeclaration[];
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
			if (clean[open] === "{" && name === "page") {
				pageRule(clean, index, context.pages ?? []);
			} else if (clean[open] === "{" && name === "media") {
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
		const selectors = split(clean.slice(index, open));
		index = end;
		let cached: Declarations | undefined;
		for (const raw of selectors) {
			const text = raw.trim();
			const where = ` in selector ${JSON.stringify(text)}${at}`;
			const compiled = selector(text, where);
			if (!compiled) continue;
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

export function parse(source: string, supplied: ParseContext | PageDeclaration[] = {}): Rule[] {
	const context: ParseContext = Array.isArray(supplied) ? { pages: supplied } : supplied;
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
