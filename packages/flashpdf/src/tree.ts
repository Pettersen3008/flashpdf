import { object, text } from "./assert.js";

const REACT_FRAGMENT = Symbol.for("react.fragment");
const FLASHPDF_FRAGMENT = Symbol.for("flashpdf.fragment");
const REACT_MEMO = Symbol.for("react.memo");
const REACT_FORWARD_REF = Symbol.for("react.forward_ref");

type Render = (props: Record<string, unknown>) => unknown;

/** `memo`/`forwardRef` wrap the real component in an object, and a class
 *  component hides it behind `prototype.render`; without unwrapping, all three
 *  reach `lower` and die as an unsupported element. */
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

/** `depth` counts element levels only, matching the renderer's own limit; the
 *  children arrays React puts between them are transparent. */
export async function reactTree(value: unknown, depth = 0): Promise<unknown> {
	if (depth > 64) throw new Error("nesting exceeds 64");
	if (typeof value === "bigint") return String(value);
	if (Array.isArray(value)) return Promise.all(value.map((child) => reactTree(child, depth)));
	if (typeof value !== "object" || value === null) return value;
	const iterator = (value as { [Symbol.iterator]?: unknown })[Symbol.iterator];
	if (typeof iterator === "function")
		return Promise.all([...(value as Iterable<unknown>)].map((child) => reactTree(child, depth)));

	const node = object(value);
	if (!("type" in node) || !("props" in node)) return value;
	const p = object(node.props);
	if (node.type === REACT_FRAGMENT || node.type === FLASHPDF_FRAGMENT)
		return reactTree(p.children, depth + 1);
	const render = component(node.type);
	if (render) return reactTree(await render(p), depth + 1);
	return {
		type: node.type,
		props: { ...p, children: await reactTree(p.children, depth + 1) },
	};
}

export function scalarText(value: unknown): string {
	let result = "";
	for (const part of children(value)) {
		if (typeof part !== "string" && typeof part !== "number")
			throw new Error("text elements only accept text children");
		result += text(part);
	}
	return result;
}

export function* children(value: unknown, depth = 0): Generator<unknown> {
	if (depth > 64) throw new Error("nesting exceeds 64");
	if (value === null || value === undefined || typeof value === "boolean") return;
	if (Array.isArray(value)) {
		for (const child of value) yield* children(child, depth + 1);
	} else yield value;
}

/** `width` and `flex` size a flex row's columns; anywhere else they would be
 *  silently dropped, so they are rejected instead. */
export function rowOnly(s: Record<string, unknown>, tag: string, inRow: boolean) {
	if (inRow) return;
	for (const key of ["width", "flex"])
		if (s[key] !== undefined)
			throw new Error(`${key} applies only to a flex row child, not <${tag}>`);
}
