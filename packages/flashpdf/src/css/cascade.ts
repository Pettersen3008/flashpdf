import type { NormalizedStyle } from "../style.js";
import { style } from "../style.js";
import type { ElementNode, Node, Tag, TextNode } from "../tree.js";
import { inline, matches, parse } from "./parser.js";
import { inherited } from "./properties.js";
import type { CascadedStyle, Identity, Rule } from "./types.js";

export type StyledElement = Omit<ElementNode, "style" | "children"> & {
	readonly style: NormalizedStyle;
	readonly children: readonly StyledNode[];
};
export type StyledNode = TextNode | StyledElement;

const uaStyles: Partial<Record<Tag, CascadedStyle>> = {
	h1: { fontSize: "2em", fontWeight: "bold" },
	h2: { fontSize: "1.5em", fontWeight: "bold" },
	h3: { fontSize: "1.17em", fontWeight: "bold" },
	h4: { fontSize: "1em", fontWeight: "bold" },
	h5: { fontSize: "0.83em", fontWeight: "bold" },
	h6: { fontSize: "0.67em", fontWeight: "bold" },
};

function cascade(target: CascadedStyle, source: CascadedStyle | undefined) {
	if (!source) return;
	for (const [key, value] of Object.entries(source)) {
		delete (target as Record<string, unknown>)[key];
		(target as Record<string, unknown>)[key] = value;
	}
}

const MAX_VARIABLE_EXPANSIONS = 1024;
const MAX_VARIABLE_OUTPUT = 64 * 1024;

function resolveVariable(
	value: string,
	variables: CascadedStyle,
	resolving = new Set<string>(),
	budget = { expansions: 0 },
): string {
	const resolved = value.replace(
		/var\((--[\w-]+)(?:,\s*([^()]+))?\)/g,
		(_, name: string, fallback: string | undefined) => {
			if (++budget.expansions > MAX_VARIABLE_EXPANSIONS)
				throw new Error("CSS variable expansion is too large");
			if (resolving.has(name)) throw new Error(`cyclic CSS variable: ${name}`);
			resolving.add(name);
			try {
				const candidate = variables[name as `--${string}`] ?? fallback ?? "";
				return typeof candidate === "string"
					? resolveVariable(candidate, variables, resolving, budget)
					: String(candidate);
			} finally {
				resolving.delete(name);
			}
		},
	);
	if (resolved.length > MAX_VARIABLE_OUTPUT) throw new Error("CSS variable output is too large");
	return resolved;
}

function resolveLengths(result: CascadedStyle, parent: CascadedStyle) {
	if (typeof result.fontSize === "string") {
		const match = /^(\d+(?:\.\d*)?|\.\d+)(pt|px|em|rem)$/.exec(result.fontSize.trim());
		if (match) {
			const parentSize = typeof parent.fontSize === "number" ? parent.fontSize : 12;
			const amount = Number(match[1]);
			result.fontSize =
				amount *
				(match[2] === "px" ? 0.75 : match[2] === "em" ? parentSize : match[2] === "rem" ? 12 : 1);
		}
	}
	if (typeof result.flex === "string" && /^(?:\d+(?:\.\d*)?|\.\d+)$/.test(result.flex.trim()))
		result.flex = Number(result.flex);
}

function visit(
	value: Node,
	rules: Rule[],
	ancestors: Identity[],
	parent: CascadedStyle,
): StyledNode {
	if (value.kind === "text") return value;
	const identity: Identity = { tag: value.tag, id: value.id, classes: value.classes };
	const result: CascadedStyle = {};
	for (const [key, item] of Object.entries(parent))
		if (key.startsWith("--") || inherited.has(key)) (result as Record<string, unknown>)[key] = item;
	cascade(result, uaStyles[value.tag]);
	for (const rule of rules)
		if (matches(rule.selector, identity, ancestors)) cascade(result, rule.declarations);
	cascade(result, inline(value.style));
	let budget: { expansions: number } | undefined;
	for (const [key, item] of Object.entries(result))
		if (typeof item === "string" && item.includes("var("))
			(result as Record<string, unknown>)[key] = resolveVariable(
				item,
				result,
				new Set(key.startsWith("--") ? [key] : []),
				(budget ??= { expansions: 0 }),
			);
	resolveLengths(result, parent);
	const resolved = style(result, value.tag);
	ancestors.push(identity);
	const children = value.children.map((child) => visit(child, rules, ancestors, result));
	ancestors.pop();
	return { ...value, style: resolved, children };
}

export function resolveStyles(
	value: readonly Node[],
	sheets: readonly string[] = [],
): readonly StyledNode[] {
	const rules = sheets.flatMap(parse);
	for (let order = 0; order < rules.length; order++) rules[order]!.order = order;
	rules.sort((a, b) => a.specificity - b.specificity || a.order - b.order);
	return value.map((node) => visit(node, rules, [], {}));
}
