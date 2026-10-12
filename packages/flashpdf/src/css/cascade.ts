import type { NormalizedStyle } from "../style.js";
import { point, style } from "../style.js";
import type { ElementNode, Node, Tag, TextNode } from "../tree.js";
import { label, locate } from "../tree.js";
import { evaluateMath } from "./math.js";
import { compareSpecificity, inline, matches, parse } from "./parser.js";
import { inherited } from "./properties.js";
import type { CascadedStyle, Elem, Registration, Rule } from "./types.js";

export type StyledElement = Omit<ElementNode, "style" | "children"> & {
	readonly style: NormalizedStyle;
	readonly children: readonly StyledNode[];
	/** `<span.total> in <div.row> > <main.invoice>`, for error messages. */
	readonly where: string;
};
export type StyledNode = TextNode | StyledElement;

// https://github.com/chromium/chromium/blob/main/third_party/blink/renderer/core/html/resources/html.css
const uaStyles: Partial<Record<Tag, CascadedStyle>> = {
	b: { fontWeight: "bold" },
	strong: { fontWeight: "bold" },
	em: { fontStyle: "italic" },
	i: { fontStyle: "italic" },
	cite: { fontStyle: "italic" },
	th: { fontWeight: "bold", textAlign: "center" },
	p: { margin: "1em 0" },
	a: { color: "#0000EE", textDecoration: "underline" },
	u: { textDecoration: "underline" },
	ins: { textDecoration: "underline" },
	s: { textDecoration: "line-through" },
	del: { textDecoration: "line-through" },
	mark: { backgroundColor: "yellow", color: "black" },
	small: { fontSize: "0.83em" },
	sub: { fontSize: "0.83em", verticalAlign: "sub" },
	sup: { fontSize: "0.83em", verticalAlign: "super" },
	code: { fontFamily: "monospace" },
	kbd: { fontFamily: "monospace" },
	samp: { fontFamily: "monospace" },
	pre: { fontFamily: "monospace", whiteSpace: "pre", margin: "1em 0" },
	blockquote: { margin: "1em 40px" },
	figure: { margin: "1em 40px" },
	dd: { marginLeft: "40px" },
	address: { fontStyle: "italic" },
	hr: { borderBottom: "1pt solid black", margin: "0.5em 0" },
	h1: { fontSize: "2em", fontWeight: "bold", breakAfter: "avoid", margin: "0.67em 0" },
	h2: { fontSize: "1.5em", fontWeight: "bold", breakAfter: "avoid", margin: "0.83em 0" },
	h3: { fontSize: "1.17em", fontWeight: "bold", breakAfter: "avoid", margin: "1em 0" },
	h4: { fontSize: "1em", fontWeight: "bold", breakAfter: "avoid", margin: "1.33em 0" },
	h5: { fontSize: "0.83em", fontWeight: "bold", breakAfter: "avoid", margin: "1.67em 0" },
	h6: { fontSize: "0.67em", fontWeight: "bold", breakAfter: "avoid", margin: "2.33em 0" },
};

/** The user-agent rules that depend on nesting: a list inside a list has no vertical margin, and
 *  an unordered one cycles through disc, circle, and square as it goes deeper. */
function listStyles(tag: Tag, ancestor: Elem | undefined): CascadedStyle | undefined {
	if (tag !== "ul" && tag !== "ol" && tag !== "dl") return undefined;
	let depth = 0;
	let nested = false;
	for (; ancestor; ancestor = ancestor.parent) {
		if (ancestor.tag === "ul" || ancestor.tag === "ol") depth++;
		if (ancestor.tag === "dl") nested = true;
	}
	nested ||= depth > 0;
	return {
		margin: nested ? "0" : "1em 0",
		...(tag === "dl"
			? {}
			: {
					paddingLeft: "40px",
					listStyleType:
						tag === "ol" ? "decimal" : ["disc", "circle", "square"][Math.min(depth, 2)]!,
				}),
	};
}

function cascade(target: CascadedStyle, source: CascadedStyle | undefined) {
	if (!source) return;
	for (const [key, value] of Object.entries(source)) {
		// React skips an undefined inline value rather than unsetting the property.
		if (value === undefined) continue;
		// Delete first so the key moves last: normalize() applies shorthands and longhands in insertion order.
		delete (target as Record<string, unknown>)[key];
		(target as Record<string, unknown>)[key] = value;
	}
}

const MAX_VARIABLE_EXPANSIONS = 1024;
const MAX_VARIABLE_OUTPUT = 64 * 1024;

/** Replaces each top-level `var(--name, fallback)`, matching parentheses so the fallback
 *  may hold `rgb(...)` or a nested `var()`. */
function replaceVariables(
	value: string,
	substitute: (name: string, fallback: string | undefined) => string,
): string {
	let result = "";
	let index = 0;
	for (let start = value.indexOf("var("); start >= 0; start = value.indexOf("var(", index)) {
		let depth = 0;
		let comma = -1;
		let end = start + 3;
		for (; end < value.length; end++) {
			if (value[end] === "(") depth++;
			else if (value[end] === ")" && --depth === 0) break;
			else if (value[end] === "," && depth === 1 && comma < 0) comma = end;
		}
		const name = value.slice(start + 4, comma < 0 ? end : comma).trim();
		if (end === value.length || !/^--[\w-]+$/.test(name))
			throw new Error(`invalid CSS variable reference: ${value.slice(start, end + 1)}`);
		const fallback = comma < 0 ? undefined : value.slice(comma + 1, end).trim();
		result += value.slice(index, start) + substitute(name, fallback);
		index = end + 1;
	}
	return result + value.slice(index);
}

function resolveVariable(
	value: string,
	variables: CascadedStyle,
	resolving = new Set<string>(),
	budget = { expansions: 0 },
): string {
	// Bounds the rescan of each nested fallback.
	if (value.length > MAX_VARIABLE_OUTPUT) throw new Error("CSS variable output is too large");
	const resolved = replaceVariables(value, (name, fallback) => {
		if (++budget.expansions > MAX_VARIABLE_EXPANSIONS)
			throw new Error("CSS variable expansion is too large");
		if (resolving.has(name)) throw new Error(`cyclic CSS variable: ${name}`);
		resolving.add(name);
		try {
			const candidate = variables[name as `--${string}`] ?? fallback;
			if (candidate === undefined)
				throw new Error(
					`undefined CSS variable: ${name} (rules inside an @layer block, such as Tailwind's :root theme, are skipped)`,
				);
			return typeof candidate === "string"
				? resolveVariable(candidate, variables, resolving, budget)
				: String(candidate);
		} finally {
			resolving.delete(name);
		}
	});
	if (resolved.length > MAX_VARIABLE_OUTPUT) throw new Error("CSS variable output is too large");
	return resolved;
}

function resolveFontSize(result: CascadedStyle, parent: NormalizedStyle) {
	// `inherit` and `initial` resolve in normalize().
	if (typeof result.fontSize !== "string" || /^\s*(?:inherit|initial)\s*$/i.test(result.fontSize))
		return;
	const parentSize = typeof parent.fontSize === "number" ? parent.fontSize : 12;
	result.fontSize = point(evaluateMath("fontSize", result.fontSize, parentSize), parentSize, true);
}

/** The element siblings of `nodes` under `parent`, which is how selectors see a tree: text nodes do not count. */
function elements(nodes: readonly Node[], parent?: Elem): Elem[] {
	const found = nodes.filter((node): node is ElementNode => node.kind === "element");
	const totals = new Map<string, number>();
	for (const { tag } of found) totals.set(tag, (totals.get(tag) ?? 0) + 1);
	const seen = new Map<string, number>();
	let previous: Elem | undefined;
	return found.map((node, index) => {
		const typeIndex = (seen.get(node.tag) ?? 0) + 1;
		seen.set(node.tag, typeIndex);
		const elem: Elem = {
			tag: node.tag,
			id: node.id,
			classes: node.classes,
			parent,
			previous,
			index: index + 1,
			count: found.length,
			typeIndex,
			typeCount: totals.get(node.tag)!,
			empty: node.children.every((child) => child.kind === "text" && child.value === ""),
		};
		previous = elem;
		return elem;
	});
}

type Computed = {
	cascaded: CascadedStyle;
	style: NormalizedStyle;
	children?: Map<unknown, Map<string, Computed>>;
};

function visit(
	value: ElementNode,
	elem: Elem,
	rules: Rule[],
	properties: ReadonlyMap<string, Registration>,
	parent: Computed,
	chain: string,
): StyledElement {
	const self = label(value);
	const where = chain ? `${self} in ${chain}` : self;
	const matched = rules.filter((rule) => matches(rule.selector, elem));
	const byStyle = (parent.children ??= new Map());
	let cache = byStyle.get(value.style);
	if (!cache) byStyle.set(value.style, (cache = new Map()));
	const list = listStyles(value.tag, elem.parent);
	const key = `${value.tag}\n${list?.margin ?? ""},${list?.listStyleType ?? ""}\n${matched.map((rule) => rule.order).join(",")}`;
	let computed = cache.get(key);
	if (!computed) {
		// No prototype, so an own `__proto__` key stays a key that style() rejects.
		const result: CascadedStyle = Object.create(null);
		let resolved: NormalizedStyle;
		try {
			for (const [key, item] of Object.entries(parent.cascaded))
				if (key.startsWith("--") ? properties.get(key)?.inherits !== false : inherited.has(key))
					(result as Record<string, unknown>)[key] = item;
			// A registered `initial-value` is the value of a property nothing sets, ahead of any var() fallback.
			for (const [name, { initial }] of properties)
				if (initial !== undefined && !(name in result))
					(result as Record<string, unknown>)[name] = initial;
			cascade(result, uaStyles[value.tag]);
			cascade(result, list);
			const own = inline(value.style);
			for (const rule of matched) cascade(result, rule.declarations().normal);
			cascade(result, own.normal);
			for (const rule of matched) cascade(result, rule.declarations().important);
			cascade(result, own.important);
			let budget: { expansions: number } | undefined;
			for (const [key, item] of Object.entries(result))
				if (typeof item === "string" && item.includes("var("))
					(result as Record<string, unknown>)[key] = resolveVariable(
						item,
						result,
						new Set(key.startsWith("--") ? [key] : []),
						(budget ??= { expansions: 0 }),
					);
			resolveFontSize(result, parent.style);
			const em =
				typeof result.fontSize === "number"
					? result.fontSize
					: typeof parent.style.fontSize === "number"
						? parent.style.fontSize
						: 12;
			for (const [key, item] of Object.entries(result))
				if (typeof item === "string" && !key.startsWith("--"))
					(result as Record<string, unknown>)[key] = evaluateMath(key, item, em);
			resolved = style(result, parent.style);
		} catch (error) {
			throw locate(error, where);
		}
		computed = { cascaded: result, style: resolved };
		cache.set(key, computed);
	}
	const siblings = elements(value.children, elem);
	let next = 0;
	const children = value.children.map((child) =>
		child.kind === "text"
			? child
			: visit(
					child,
					siblings[next++]!,
					rules,
					properties,
					computed,
					chain ? `${self} > ${chain}` : self,
				),
	);
	return { ...value, style: computed.style, where, children };
}

export function styleResolver(
	sheets: readonly string[] = [],
	width = 523,
): (value: readonly Node[]) => readonly StyledNode[] {
	const properties = new Map<string, Registration>();
	let rules: Rule[] | undefined;
	const root: Computed = { cascaded: {}, style: {} };
	return (value) => {
		if (!rules) {
			rules = sheets.flatMap((sheet) => parse(sheet, { width, properties }));
			for (let order = 0; order < rules.length; order++) rules[order]!.order = order;
			rules.sort((a, b) => compareSpecificity(a.specificity, b.specificity) || a.order - b.order);
		}
		const roots = elements(value);
		let next = 0;
		return value.map((node) =>
			node.kind === "text" ? node : visit(node, roots[next++]!, rules!, properties, root, ""),
		);
	};
}

export function resolveStyles(
	value: readonly Node[],
	sheets: readonly string[] = [],
	width = 523,
): readonly StyledNode[] {
	return styleResolver(sheets, width)(value);
}
