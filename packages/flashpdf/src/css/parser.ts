import type { Style } from "../element.js";
import { property } from "./properties.js";
import type { CascadedStyle, Identity, Rule, SelectorPart, SelectorPiece } from "./types.js";

function declarations(source: string, context = ""): CascadedStyle {
	const result: Record<string, string> = {};
	for (const item of source.split(";")) {
		if (!item.trim()) continue;
		const colon = item.indexOf(":");
		if (colon < 1) throw new Error(`invalid CSS declaration: ${item.trim()}${context}`);
		const key = property(item.slice(0, colon).trim(), context);
		delete result[key];
		result[key] = item.slice(colon + 1).trim();
	}
	return result as CascadedStyle;
}

function selectorPart(part: string): SelectorPart {
	return {
		tag: part.match(/^[a-z][\w-]*/i)?.[0] ?? "",
		ids: part.match(/#[\w-]+/g)?.map((value) => value.slice(1)) ?? [],
		classes: part.match(/\.[\w-]+/g)?.map((value) => value.slice(1)) ?? [],
	};
}
function specificity(selector: readonly SelectorPiece[]) {
	let result = 0;
	for (const part of selector)
		if (typeof part !== "string")
			result += part.ids.length * 100 + part.classes.length * 10 + Number(Boolean(part.tag));
	return result;
}
function matchesPart(part: SelectorPiece | undefined, node: Identity | undefined) {
	if (part === undefined || typeof part === "string" || node === undefined) return false;
	return (
		(!part.tag || part.tag === node.tag) &&
		part.ids.every((id) => id === node.id) &&
		part.classes.every((value) => node.classes.includes(value))
	);
}
export function matches(
	pieces: readonly SelectorPiece[],
	node: Identity,
	ancestors: readonly Identity[],
) {
	if (!matchesPart(pieces.at(-1), node)) return false;
	let ancestor = ancestors.length - 1;
	for (let index = pieces.length - 2; index > 0; index -= 2) {
		const wanted = pieces[index - 1];
		if (pieces[index] === ">") {
			if (!matchesPart(wanted, ancestors[ancestor--])) return false;
		} else {
			while (ancestor >= 0 && !matchesPart(wanted, ancestors[ancestor])) ancestor--;
			if (ancestor-- < 0) return false;
		}
	}
	return true;
}

function position(source: string, index: number) {
	const before = source.slice(0, index);
	return `${before.split("\n").length}:${before.length - before.lastIndexOf("\n")}`;
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
export function parse(source: string): Rule[] {
	const rules: Rule[] = [];
	const clean = withoutComments(source);
	const at = clean.indexOf("@");
	if (at >= 0)
		throw new Error(
			`unsupported CSS at-rule: ${clean.slice(at).split(/[\s{;]/)[0]} at ${position(clean, at)}`,
		);
	let end = 0;
	while (end < clean.length) {
		const start = clean.indexOf("{", end);
		if (start < 0) break;
		if (clean.slice(end, start).includes("}")) break;
		const close = clean.indexOf("}", start + 1);
		const nested = clean.indexOf("{", start + 1);
		if (close < 0 || (nested >= 0 && nested < close)) break;
		if (start === end) throw new Error(`invalid CSS stylesheet at ${position(clean, end)}`);
		const at = ` at ${position(clean, end)}`;
		const selectors = clean.slice(end, start);
		const body = clean.slice(start + 1, close);
		end = close + 1;
		for (const selector of selectors.split(",").map((value) => value.trim())) {
			const normalized = normalizeSelector(selector);
			const pieces = normalized.split(/(>| )/).filter(Boolean);
			const validPart = (part: string) =>
				/^(?:[a-z][\w-]*)?(?:(?:#[\w-]+)|(?:\.[\w-]+))*$/i.test(part) && part !== "";
			if (
				!selector ||
				!pieces.length ||
				pieces.some((part, index) =>
					index % 2 === 0 ? !validPart(part) : part !== " " && part !== ">",
				) ||
				pieces.length % 2 === 0
			)
				throw new Error(`unsupported CSS selector: ${selector || selectors.trim()}${at}`);
			const compiled = pieces.map((part, index) =>
				index % 2 === 0 ? selectorPart(part) : (part as " " | ">"),
			);
			rules.push({
				selector: compiled,
				declarations: declarations(body, ` in selector ${JSON.stringify(selector)}${at}`),
				order: rules.length,
				specificity: specificity(compiled),
			});
		}
	}
	if (clean.slice(end).trim()) throw new Error(`invalid CSS stylesheet at ${position(clean, end)}`);
	return rules;
}

export function inline(value: Style | string | undefined): CascadedStyle {
	if (value === undefined) return {};
	return typeof value === "string" ? declarations(value, " in inline style") : value;
}
