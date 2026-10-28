import { object, props, text } from "./assert.js";
import type { Style } from "./element.js";
import { PageToken } from "./page.js";
import { isMarkup, markup, vector, type Drawing } from "./svg.js";

/** The style keys the core supports (css/properties.rs); it rejects any other key by name. */
const supported = new Set(
	`
	display flexDirection flexWrap justifyContent alignItems alignSelf alignContent flex flexGrow flexShrink
	flexBasis width minWidth maxWidth height minHeight maxHeight boxSizing margin marginTop
	marginRight marginBottom marginLeft padding paddingTop paddingRight paddingBottom paddingLeft
	border borderWidth borderColor borderTop borderRight borderBottom borderLeft borderStyle
	borderTopWidth borderRightWidth borderBottomWidth borderLeftWidth borderTopColor
	borderRightColor borderBottomColor borderLeftColor borderTopStyle borderRightStyle
	borderBottomStyle borderLeftStyle borderRadius borderTopLeftRadius borderTopRightRadius
	borderBottomRightRadius borderBottomLeftRadius opacity marginInline marginBlock paddingInline
	paddingBlock background backgroundColor gap rowGap columnGap fontSize fontFamily fontWeight fontStyle
	fontStretch fontVariationSettings fontKerning textAlign direction textTransform textDecoration textDecorationColor whiteSpace verticalAlign
	listStyle listStyleType listStylePosition color lineHeight letterSpacing wordSpacing
	breakBefore breakAfter breakInside borderCollapse borderSpacing orphans widows
	position top right bottom left inset insetInline insetBlock zIndex transform transformOrigin transformBox clipPath overflow`
		.trim()
		.split(/\s+/),
);

const REACT_FRAGMENT = Symbol.for("react.fragment");
const FLASHPDF_FRAGMENT = Symbol.for("flashpdf.fragment");
const REACT_MEMO = Symbol.for("react.memo");
const REACT_FORWARD_REF = Symbol.for("react.forward_ref");
// React 19 makes the context object its own provider; React 18 wraps it in a react.provider object.
const REACT_CONTEXT = Symbol.for("react.context");
const REACT_PROVIDER = Symbol.for("react.provider");

type Render = (props: Record<string, unknown>) => unknown;
type Component = { render: Render; name: string };

export const blockTags = [
	"main",
	"div",
	"section",
	"article",
	"header",
	"footer",
	"ul",
	"ol",
	"li",
	"dl",
	"dt",
	"dd",
	"blockquote",
	"address",
	"figure",
	"figcaption",
	"nav",
	"aside",
] as const;
export const textTags = [
	"p",
	"span",
	"b",
	"strong",
	"em",
	"i",
	"cite",
	"small",
	"sub",
	"sup",
	"u",
	"ins",
	"s",
	"del",
	"mark",
	"code",
	"kbd",
	"samp",
	"pre",
	"h1",
	"h2",
	"h3",
	"h4",
	"h5",
	"h6",
] as const;
export const tableTags = ["table", "thead", "tbody", "tfoot", "tr", "th", "td"] as const;
export const tags = [
	...blockTags,
	...textTags,
	...tableTags,
	"a",
	"img",
	"svg",
	"hr",
	"br",
] as const;
export type BlockTag = (typeof blockTags)[number];
export type TextTag = (typeof textTags)[number];
export type TableTag = (typeof tableTags)[number];
export type Tag = (typeof tags)[number];

export type TextNode = { readonly kind: "text"; readonly value: string };
export type ElementNode = {
	readonly kind: "element";
	readonly tag: Tag;
	readonly id?: string | undefined;
	readonly classes: readonly string[];
	readonly style?: Style | string | undefined;
	readonly children: readonly Node[];
	readonly src?: Uint8Array | undefined;
	readonly alt?: string | undefined;
	readonly href?: string | undefined;
	/** An `<svg>`, or an `<img>` whose `src` is SVG markup. */
	readonly drawing?: Drawing | undefined;
	/** `data-bookmark` on a heading: its outline title, or `false` to leave it out. */
	readonly bookmark?: string | false | undefined;
	/** `<ol start>`, `<ol reversed>`, and `<li value>`. */
	readonly start?: number | undefined;
	readonly reversed?: boolean | undefined;
	readonly value?: number | undefined;
	readonly colSpan?: number | undefined;
	readonly rowSpan?: number | undefined;
	readonly dir?: "ltr" | "rtl" | "auto" | undefined;
};
export type Node = TextNode | ElementNode;

const HINTS: Record<string, string> = {
	colgroup: "each column's width comes from its first <th>/<td> without colSpan",
	col: "each column's width comes from its first <th>/<td> without colSpan",
	caption: "captions are not supported; use a <p> before the table",
};

export function label(node: {
	tag: string;
	id?: string | undefined;
	classes: readonly string[];
}): string {
	return `<${node.tag}${node.id ? `#${node.id}` : ""}${node.classes.map((c) => `.${c}`).join("")}>`;
}

const located = new WeakSet<Error>();
/** Appends the element path once. */
export function locate(error: unknown, where: string): unknown {
	if (!(error instanceof Error) || located.has(error)) return error;
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
		const child = normalize(value, depth, true);
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

/** `depth` counts element levels, matching the renderer's own limit. The children
 *  array React puts under an element is free; an array nested in an array (`nested`)
 *  counts, so deep arrays cannot overflow the stack. */
function normalize(value: unknown, depth: number, nested = false): unknown {
	if (depth > 64) throw new Error("nesting exceeds 64");
	if (typeof value === "bigint") return String(value);
	if (Array.isArray(value)) return list(value, depth + Number(nested));
	if (typeof value !== "object" || value === null) return value;
	const iterator = (value as { [Symbol.iterator]?: unknown })[Symbol.iterator];
	if (typeof iterator === "function")
		return list(value as Iterable<unknown>, depth + Number(nested));

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

/** Normalises through WHATWG URL parsing, which also percent-encodes non-ASCII. */
export function href(value: unknown, where = ""): string {
	if (typeof value !== "string" || !value)
		throw new Error(`href must be an absolute http, https, or mailto URL${where}`);
	if (value.startsWith("#")) {
		if (!/^#[\x21-\x7e]{1,255}$/.test(value))
			throw new Error(
				`a link to an id is # and 1 to 255 printable ASCII characters, not ${JSON.stringify(value)}${where}`,
			);
		return value;
	}
	let url: URL;
	try {
		url = new URL(value);
	} catch {
		throw new Error(
			`href must be an absolute http, https, or mailto URL, not ${JSON.stringify(value)}${where}`,
		);
	}
	if (!["http:", "https:", "mailto:"].includes(url.protocol))
		throw new Error(
			`href scheme ${url.protocol} is not allowed; use http, https, or mailto${where}`,
		);
	// WHATWG parses `http:foo` as `http://foo/`; an absolute web URL must spell out its host.
	if (url.protocol !== "mailto:" && !/^\s*https?:\/\/[^/\\?#\s]/i.test(value))
		throw new Error(
			`href must name a host, such as https://example.com/, not ${JSON.stringify(value)}${where}`,
		);
	if (!/^[\x21-\x7e]+$/.test(url.href))
		throw new Error(
			`href must be printable ASCII; percent-encode ${JSON.stringify(url.href)}${where}`,
		);
	return url.href;
}

/** `data-bookmark` is a heading's outline title, or `false` to leave it out. */
function bookmark(value: unknown, where: string): { bookmark?: string | false } {
	if (value === undefined) return {};
	if (
		value !== false &&
		(typeof value !== "string" ||
			!value.trim() ||
			[...value].length > 255 ||
			/[\p{Cc}\ud800-\udfff]/u.test(value))
	)
		throw new Error(
			`data-bookmark must be 1 to 255 characters without control characters, or false${where}`,
		);
	return { bookmark: value as string | false };
}

export function image(input: Record<string, unknown>, where: string, chain: string, depth: number) {
	const { src, alt } = input;
	if (typeof src === "string")
		throw new Error(
			`src must be the image's PNG, JPEG, or SVG bytes as a Uint8Array, not a URL, path, or data: URL; read or fetch the file and pass its bytes${where}`,
		);
	if (!(src instanceof Uint8Array) || !src.byteLength)
		throw new Error(`src must be a non-empty Uint8Array of PNG, JPEG, or SVG bytes${where}`);
	if (alt !== undefined && (typeof alt !== "string" || /\p{Cc}/u.test(alt)))
		throw new Error(`alt must be a string without control characters${where}`);
	if (input.children !== undefined && input.children !== null)
		throw new Error(`<img> has no children${where}`);
	if (!isMarkup(src)) return { src, alt };
	let root;
	try {
		root = markup(src);
	} catch (error) {
		if (error instanceof Error) error.message += where;
		throw error;
	}
	const svg = vector(root.props, chain ? `<img> > ${chain}` : "<img>", depth + 1);
	return { drawing: svg.drawing, alt: alt ?? svg.alt };
}

/** An `ol` `start` or `li` `value`: a 32-bit integer, as a number or an integer string. */
function ordinal(value: unknown, name: string, where: string): number | undefined {
	if (value === undefined) return undefined;
	const number =
		typeof value === "string" && /^[+-]?\d+$/.test(value.trim()) ? Number(value) : value;
	if (typeof number !== "number" || !Number.isInteger(number) || Math.abs(number) > 2 ** 31 - 1)
		throw new Error(`${name} must be an integer${where}`);
	return number;
}

function reversed(value: unknown, where: string): boolean | undefined {
	if (value !== undefined && typeof value !== "boolean")
		throw new Error(`reversed must be a boolean${where}`);
	return value;
}

/** 256 columns is the protocol's row limit; 65535 rows fit its u16 row span. */
function span(value: unknown, name: string, max: number, where: string) {
	if (value === undefined || value === null) return undefined;
	if (!Number.isInteger(value) || (value as number) < 1 || (value as number) > max)
		throw new Error(`${name} must be an integer from 1 to ${max}${where}`);
	return value as number;
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

function host(value: unknown, depth = 0, chain = "", nested = false): Node[] {
	if (depth > 64) throw new Error("nesting exceeds 64");
	if (value === null || value === undefined || typeof value === "boolean") return [];
	if (typeof value === "string" || typeof value === "number")
		return [{ kind: "text", value: text(value) }];
	if (value instanceof PageToken) return [{ kind: "text", value: value.token }];
	if (Array.isArray(value))
		return value.flatMap((child) => host(child, depth + Number(nested), chain, true));

	const inside = chain ? ` in ${chain}` : "";
	if (typeof value !== "object" || !("type" in value) || !("props" in value)) {
		const kind =
			typeof value === "object"
				? `${Object.prototype.toString.call(value).slice(8, -1)} object`
				: typeof value;
		throw new Error(`unsupported child: a ${kind}${inside}; pass a string, number, or element`);
	}
	const element = object(value);
	if (!isTag(element.type)) {
		const hint = typeof element.type === "string" ? HINTS[element.type] : undefined;
		throw new Error(
			`unsupported element ${describe(element.type)}${inside}${hint ? `: ${hint}` : ""}`,
		);
	}
	const where = ` on <${element.type}>${inside}`;
	const cell = element.type === "td" || element.type === "th";
	// React 19 passes `ref` as a plain prop; a host element has nothing to attach it to.
	const heading = /^h[1-6]$/.test(element.type);
	const own =
		element.type === "img"
			? ["src", "alt"]
			: element.type === "a"
				? ["href"]
				: element.type === "svg"
					? Object.keys(object(element.props))
					: heading
						? ["data-bookmark"]
						: element.type === "ol"
							? ["start", "reversed"]
							: element.type === "li"
								? ["value"]
								: cell
									? ["colSpan", "rowSpan"]
									: [];
	const input = props(element.props, [
		"children",
		"id",
		"className",
		"style",
		"ref",
		"dir",
		...own,
	]);
	if (input.dir !== undefined && input.dir !== "ltr" && input.dir !== "rtl" && input.dir !== "auto")
		throw new Error(`dir must be "ltr", "rtl", or "auto"${where}`);
	if ((element.type === "hr" || element.type === "br") && input.children != null)
		throw new Error(`<${element.type}> has no children${where}`);
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
	// An unknown key keeps its own message from the core, whatever its value.
	if (typeof input.style === "object")
		for (const [key, value] of Object.entries(input.style as object))
			if (
				(supported.has(key) || key.startsWith("--")) &&
				value !== undefined &&
				typeof value !== "string" &&
				typeof value !== "number"
			)
				throw new Error(
					`style ${key} must be a string or number, not ${value === null ? "null" : typeof value}${where}`,
				);

	const node: ElementNode = {
		kind: "element",
		tag: element.type,
		id: input.id,
		classes:
			typeof input.className === "string" ? input.className.split(/\s+/).filter(Boolean) : [],
		style: input.style as Style | string | undefined,
		...(input.dir !== undefined ? { dir: input.dir as "ltr" | "rtl" | "auto" } : {}),
		children: [],
		...(element.type === "img" ? image(input, where, chain, depth) : {}),
		...(element.type === "a" ? { href: href(input.href, where) } : {}),
		...(element.type === "svg" ? vector(input, chain, depth) : {}),
		...(heading ? bookmark(input["data-bookmark"], where) : {}),
		...(element.type === "ol"
			? { start: ordinal(input.start, "start", where), reversed: reversed(input.reversed, where) }
			: {}),
		...(element.type === "li" ? { value: ordinal(input.value, "value", where) } : {}),
		...(cell
			? {
					colSpan: span(input.colSpan, "colSpan", 256, where),
					rowSpan: span(input.rowSpan, "rowSpan", 65535, where),
				}
			: {}),
	};
	const self = label(node);
	// An svg's children became its drawing.
	const children =
		element.type === "svg"
			? []
			: host(input.children, depth + 1, chain ? `${self} > ${chain}` : self);
	return [{ ...node, children }];
}

/** The React-compatible resolver owns all dynamic input. Everything after this
 * boundary receives only supported host elements and validated text values. */
export async function resolveTree(value: unknown): Promise<readonly Node[]> {
	return host(await reactTree(value));
}
