import { object, props, text } from "./assert.js";
import type { Style } from "./element.js";
import type { NormalizedStyle } from "./style.js";

const REACT_FRAGMENT = Symbol.for("react.fragment");
const FLASHPDF_FRAGMENT = Symbol.for("flashpdf.fragment");
const REACT_MEMO = Symbol.for("react.memo");
const REACT_FORWARD_REF = Symbol.for("react.forward_ref");

type Render = (props: Record<string, unknown>) => unknown;

export const tags = [
	"main",
	"div",
	"section",
	"article",
	"header",
	"footer",
	"p",
	"span",
	"h1",
	"h2",
	"h3",
	"h4",
	"h5",
	"h6",
	"hr",
] as const;
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

/** `memo`/`forwardRef` wrap the real component in an object, and a class
 *  component hides it behind `prototype.render`; without unwrapping, all three
 *  reach the host-node boundary as unsupported elements. */
function component(type: unknown): Render | undefined {
	if (typeof type === "function") {
		const klass = type as { prototype?: { isReactComponent?: unknown } };
		if (!klass.prototype?.isReactComponent) return type as Render;
		const Component = type as new (props: Record<string, unknown>) => { render(): unknown };
		return (p) => new Component(p).render();
	}
	if (typeof type !== "object" || type === null) return undefined;
	const wrapper = type as { $$typeof?: unknown; type?: unknown; render?: unknown };
	if (wrapper.$$typeof === REACT_MEMO) return component(wrapper.type);
	if (wrapper.$$typeof === REACT_FORWARD_REF && typeof wrapper.render === "function")
		return (p) => {
			const { ref, ...props } = p;
			return (wrapper.render as (props: Record<string, unknown>, ref: unknown) => unknown)(
				props,
				ref,
			);
		};
	return undefined;
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
	if (node.type === REACT_FRAGMENT || node.type === FLASHPDF_FRAGMENT)
		return normalize(p.children, depth + 1);
	const render = component(node.type);
	if (render) return Promise.resolve(render(p)).then((result) => normalize(result, depth + 1));
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

function host(value: unknown, depth = 0): Node[] {
	if (depth > 64) throw new Error("nesting exceeds 64");
	if (value === null || value === undefined || typeof value === "boolean") return [];
	if (typeof value === "string" || typeof value === "number")
		return [{ kind: "text", value: text(value) }];
	if (Array.isArray(value)) return value.flatMap((child) => host(child, depth));

	const element = object(value);
	if (!isTag(element.type)) throw new Error("unsupported element");
	const input = props(element.props, ["children", "id", "className", "style"]);
	if (input.id !== undefined && typeof input.id !== "string")
		throw new Error("id must be a string");
	if (input.className !== undefined && typeof input.className !== "string")
		throw new Error("className must be a string");
	if (
		input.style !== undefined &&
		typeof input.style !== "string" &&
		(typeof input.style !== "object" || input.style === null || Array.isArray(input.style))
	)
		throw new Error("style must be an object or string");

	return [
		{
			kind: "element",
			tag: element.type,
			id: input.id,
			classes:
				typeof input.className === "string" ? input.className.split(/\s+/).filter(Boolean) : [],
			style: input.style as Style | string | undefined,
			children: host(input.children, depth + 1),
		},
	];
}

/** The React-compatible resolver owns all dynamic input. Everything after this
 * boundary receives only supported host elements and validated text values. */
export async function resolveTree(value: unknown): Promise<readonly Node[]> {
	return host(await reactTree(value));
}

/** `width` and `flex` size a flex row's columns; anywhere else they would be
 *  silently dropped, so they are rejected instead. */
export function rowOnly(s: NormalizedStyle, tag: string, inRow: boolean) {
	if (inRow) return;
	for (const key of ["width", "flex"] as const)
		if (s[key] !== undefined)
			throw new Error(`${key} applies only to a flex row child, not <${tag}>`);
}
