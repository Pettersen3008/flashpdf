import { object, props, text } from "./assert.js";
import type { Style } from "./element.js";
import type { NormalizedStyle } from "./style.js";

const REACT_FRAGMENT = Symbol.for("react.fragment");
const FLASHPDF_FRAGMENT = Symbol.for("flashpdf.fragment");
const REACT_MEMO = Symbol.for("react.memo");
const REACT_FORWARD_REF = Symbol.for("react.forward_ref");
// React 19 makes the context object its own provider; React 18 wraps it in a react.provider object.
const REACT_CONTEXT = Symbol.for("react.context");
const REACT_PROVIDER = Symbol.for("react.provider");

type Render = (props: Record<string, unknown>) => unknown;
type Component = { render: Render; name: string };

export const blockTags = ["main", "div", "section", "article", "header", "footer"] as const;
export const textTags = ["p", "span", "h1", "h2", "h3", "h4", "h5", "h6"] as const;
export const tags = [...blockTags, ...textTags, "hr"] as const;
export type BlockTag = (typeof blockTags)[number];
export type TextTag = (typeof textTags)[number];
export type Tag = (typeof tags)[number];

export type TextNode = { readonly kind: "text"; readonly value: string };
export type ElementNode = {
	readonly kind: "element";
	readonly tag: Tag;
	readonly id?: string | undefined;
	readonly classes: readonly string[];
	readonly style?: Style | string | undefined;
	readonly children: readonly Node[];
};
export type Node = TextNode | ElementNode;

const HINTS: Record<string, string> = {
	b: "use <span style={{ fontWeight: 'bold' }}>",
	strong: "use <span style={{ fontWeight: 'bold' }}>",
	em: "italic faces are not supported yet; use <span>",
	i: "italic faces are not supported yet; use <span>",
	img: "images are not supported yet",
	table: "tables are not supported yet; use flex rows of <div> and <span>",
	ul: "lists are not supported yet; use one <p> per item",
	ol: "lists are not supported yet; use one <p> per item",
	li: "lists are not supported yet; use one <p> per item",
	a: "links are not supported; use <span> for the label",
	br: "line breaks are not supported; use one <p> per line",
};

export function label(node: {
	tag: string;
	id?: string | undefined;
	classes: readonly string[];
}): string {
	return `<${node.tag}${node.id ? `#${node.id}` : ""}${node.classes.map((c) => `.${c}`).join("")}>`;
}

const located = new WeakSet<Error>();
/** Appends the element path once; a bare PageOverflow stays bare so `box()` can still recognise it. */
export function locate(error: unknown, where: string): unknown {
	if (!(error instanceof Error) || located.has(error) || error.message === "PageOverflow")
		return error;
	error.message += ` on ${where}`;
	located.add(error);
	return error;
}

function displayName(type: { displayName?: unknown; name?: unknown }): string {
	if (typeof type.displayName === "string" && type.displayName) return type.displayName;
	return typeof type.name === "string" && type.name ? type.name : "Anonymous";
}

/** `memo`/`forwardRef` wrap the real component in an object, and a class
 *  component hides it behind `prototype.render`; without unwrapping, all three
 *  reach the host-node boundary as unsupported elements. */
function component(type: unknown): Component | undefined {
	if (typeof type === "function") {
		const klass = type as { prototype?: { isReactComponent?: unknown } };
		const name = displayName(type as { displayName?: unknown; name?: unknown });
		if (!klass.prototype?.isReactComponent) return { render: type as Render, name };
		const Component = type as new (props: Record<string, unknown>) => { render(): unknown };
		return { render: (p) => new Component(p).render(), name };
	}
	if (typeof type !== "object" || type === null) return undefined;
	const wrapper = type as { $$typeof?: unknown; type?: unknown; render?: unknown };
	if (wrapper.$$typeof === REACT_MEMO) return component(wrapper.type);
	if (wrapper.$$typeof === REACT_FORWARD_REF && typeof wrapper.render === "function")
		return {
			name: displayName({
				displayName: (wrapper as { displayName?: unknown }).displayName,
				name: (wrapper.render as { name?: unknown }).name,
			}),
			render: (p) => {
				const { ref, ...props } = p;
				return (wrapper.render as (props: Record<string, unknown>, ref: unknown) => unknown)(
					props,
					ref,
				);
			},
		};
	return undefined;
}

function isProvider(type: unknown) {
	const kind = (type as { $$typeof?: unknown } | null)?.$$typeof;
	return typeof type === "object" && (kind === REACT_CONTEXT || kind === REACT_PROVIDER);
}

function thenable(value: unknown): value is PromiseLike<unknown> {
	return (
		(typeof value === "object" || typeof value === "function") &&
		value !== null &&
		typeof (value as { then?: unknown }).then === "function"
	);
}

function list(values: Iterable<unknown>, depth: number): unknown[] | Promise<unknown[]> {
	const result: unknown[] = [];
	let pending = false;
	let unchanged = Array.isArray(values);
	for (const value of values) {
		const child = normalize(value, depth);
		pending ||= thenable(child);
		unchanged &&= child === value;
		result.push(child);
	}
	return pending ? Promise.all(result) : unchanged ? (values as unknown[]) : result;
}

function invoke(component: Component, p: Record<string, unknown>, depth: number) {
	const failed = (error: unknown): never => {
		throw new Error(
			`<${component.name}> threw while rendering: ${error instanceof Error ? error.message : String(error)}. FlashPDF resolves pure function components without React; hooks and context values are not available.`,
			{ cause: error },
		);
	};
	let result: unknown;
	try {
		result = component.render(p);
	} catch (error) {
		failed(error);
	}
	return Promise.resolve(result).then((value) => normalize(value, depth + 1), failed);
}

/** `depth` counts element levels only, matching the renderer's own limit; the
 *  children arrays React puts between them are transparent. */
function normalize(value: unknown, depth: number): unknown {
	if (depth > 64) throw new Error("nesting exceeds 64");
	if (typeof value === "bigint") return String(value);
	if (Array.isArray(value)) return list(value, depth);
	if (typeof value !== "object" || value === null) return value;
	const iterator = (value as { [Symbol.iterator]?: unknown })[Symbol.iterator];
	if (typeof iterator === "function") return list(value as Iterable<unknown>, depth);

	const node = object(value);
	if (!("type" in node) || !("props" in node)) return value;
	const p = object(node.props);
	if (node.type === REACT_FRAGMENT || node.type === FLASHPDF_FRAGMENT || isProvider(node.type))
		return normalize(p.children, depth + 1);
	const render = component(node.type);
	if (render) return invoke(render, p, depth);
	const child = normalize(p.children, depth + 1);
	const result = (children: unknown) => ({
		type: node.type,
		props: { ...p, children },
	});
	return thenable(child)
		? Promise.resolve(child).then(result)
		: child === p.children && !("$$typeof" in node)
			? value
			: result(child);
}

export async function reactTree(value: unknown, depth = 0): Promise<unknown> {
	return normalize(value, depth);
}

function isTag(value: unknown): value is Tag {
	return typeof value === "string" && (tags as readonly string[]).includes(value);
}

function describe(type: unknown) {
	if (typeof type === "string") return `<${type}>`;
	if (typeof type === "object" && type !== null)
		return String((type as { $$typeof?: unknown }).$$typeof ?? "object");
	return String(type);
}

function host(value: unknown, depth = 0, chain = ""): Node[] {
	if (depth > 64) throw new Error("nesting exceeds 64");
	if (value === null || value === undefined || typeof value === "boolean") return [];
	if (typeof value === "string" || typeof value === "number")
		return [{ kind: "text", value: text(value) }];
	if (Array.isArray(value)) return value.flatMap((child) => host(child, depth, chain));

	const element = object(value);
	const inside = chain ? ` in ${chain}` : "";
	if (!isTag(element.type)) {
		const hint = typeof element.type === "string" ? HINTS[element.type] : undefined;
		throw new Error(
			`unsupported element ${describe(element.type)}${inside}${hint ? `: ${hint}` : ""}`,
		);
	}
	const where = ` on <${element.type}>${inside}`;
	// React 19 passes `ref` as a plain prop; a host element has nothing to attach it to.
	const input = props(element.props, ["children", "id", "className", "style", "ref"]);
	if (input.id !== undefined && typeof input.id !== "string")
		throw new Error(`id must be a string${where}`);
	if (input.className !== undefined && typeof input.className !== "string")
		throw new Error(`className must be a string${where}`);
	if (
		input.style !== undefined &&
		typeof input.style !== "string" &&
		(typeof input.style !== "object" || input.style === null || Array.isArray(input.style))
	)
		throw new Error(`style must be an object or string${where}`);

	const node: ElementNode = {
		kind: "element",
		tag: element.type,
		id: input.id,
		classes:
			typeof input.className === "string" ? input.className.split(/\s+/).filter(Boolean) : [],
		style: input.style as Style | string | undefined,
		children: [],
	};
	const self = label(node);
	return [
		{ ...node, children: host(input.children, depth + 1, chain ? `${self} > ${chain}` : self) },
	];
}

/** The React-compatible resolver owns all dynamic input. Everything after this
 * boundary receives only supported host elements and validated text values. */
export async function resolveTree(value: unknown): Promise<readonly Node[]> {
	return host(await reactTree(value));
}

/** `width` and `flex` size a flex row's columns; anywhere else they would be
 *  silently dropped, so they are rejected instead. */
export function rowOnly(s: NormalizedStyle, inRow: boolean) {
	if (inRow) return;
	for (const key of ["width", "flex"] as const)
		if (s[key] !== undefined) throw new Error(`${key} applies only to a flex row child`);
}
