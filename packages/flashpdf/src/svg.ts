import { object } from "./assert.js";
import { color } from "./color.js";

type Rgb = readonly [number, number, number];
type Matrix = readonly [number, number, number, number, number, number];
/** `null` paints nothing. */
export type Paint = Rgb | "currentColor" | null;

type State = {
	matrix: Matrix;
	fill: Paint;
	evenOdd: boolean;
	fillOpacity: number;
	stroke: Paint;
	strokeWidth: number;
	cap: number;
	join: number;
	miter: number;
	dashes: readonly number[];
	dashOffset: number;
	strokeOpacity: number;
	/** The product of every ancestor's `opacity`, applied to fill and stroke separately. */
	opacity: number;
};
/** 0 move, 1 line, 2 cubic, 3 close; `points` holds their coordinates in order. */
export type Shape = State & { readonly ops: readonly number[]; readonly points: readonly number[] };

export type Drawing = {
	/** Intrinsic size in points. */
	readonly width: number;
	readonly height: number;
	readonly viewBox: readonly [number, number, number, number];
	/** x and y alignment factors; undefined is `preserveAspectRatio="none"`. */
	readonly align: readonly [number, number] | undefined;
	readonly slice: boolean;
	readonly shapes: readonly Shape[];
};

const MAX_ELEMENTS = 10_000;
/** The engine's cap per drawing; a version 40 QR code needs under 80,000. */
const MAX_SEGMENTS = 100_000;
const MAX_MARKUP = 4 * 1024 * 1024;
const KAPPA = 0.5522847498307936;
const IDENTITY: Matrix = [1, 0, 0, 1, 0, 0];
const INITIAL: State = {
	matrix: IDENTITY,
	fill: [0, 0, 0],
	evenOdd: false,
	fillOpacity: 1,
	stroke: null,
	strokeWidth: 1,
	cap: 0,
	join: 0,
	miter: 4,
	dashes: [],
	dashOffset: 0,
	strokeOpacity: 1,
	opacity: 1,
};

const TEXT = "SVG text is not supported; convert it to paths or put a <p> beside the drawing";
const GRADIENT = "gradients and patterns are not supported; use a solid fill";
const REFERENCE = "use, symbol, and defs are not supported; inline the shapes";
const REJECTED: Record<string, string> = {
	text: TEXT,
	tspan: TEXT,
	textPath: TEXT,
	linearGradient: GRADIENT,
	radialGradient: GRADIENT,
	pattern: GRADIENT,
	filter: "filters are not supported",
	mask: "masks are not supported",
	clipPath: "clipPath is not supported; the drawing is clipped to its box",
	use: REFERENCE,
	symbol: REFERENCE,
	defs: REFERENCE,
	image: "<image> inside <svg> is not supported; put an <img> beside the drawing",
	foreignObject: "foreignObject is not supported",
	style: "CSS inside <svg> is not supported; use presentation attributes or a style object",
};
const GEOMETRY: Record<string, readonly string[]> = {
	g: [],
	rect: ["x", "y", "width", "height", "rx", "ry"],
	circle: ["cx", "cy", "r"],
	ellipse: ["cx", "cy", "rx", "ry"],
	line: ["x1", "y1", "x2", "y2"],
	polyline: ["points"],
	polygon: ["points"],
	path: ["d"],
};

const NUMBER = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$/i;
const invalid = (name: string, value: unknown) => new Error(`invalid ${name}: ${String(value)}`);

/** A user-space number; `px` is the same unit. */
function number(value: unknown, name: string): number {
	const source =
		typeof value === "number" ? String(value) : typeof value === "string" ? value.trim() : "";
	const text = source.endsWith("px") ? source.slice(0, -2) : source;
	if (!NUMBER.test(text) || !Number.isFinite(Math.fround(Number(text)))) throw invalid(name, value);
	return Number(text);
}

function size(value: unknown, name: string): number {
	const result = number(value, name);
	if (result < 0) throw invalid(name, value);
	return result;
}

function list(value: string, name: string): number[] {
	const source = value.trim();
	return source ? source.split(/\s*,\s*|\s+/).map((item) => number(item, name)) : [];
}

function unit(value: unknown, name: string): number {
	const percent = typeof value === "string" && value.trim().endsWith("%");
	const amount = number(percent ? (value as string).trim().slice(0, -1) : value, name);
	return Math.min(1, Math.max(0, percent ? amount / 100 : amount));
}

function keyword(value: unknown, name: string, options: readonly string[]): number {
	const index = typeof value === "string" ? options.indexOf(value.trim()) : -1;
	if (index < 0) throw new Error(`invalid ${name}: ${String(value)} (use ${options.join(", ")})`);
	return index;
}

function paint(value: unknown, name: string): Paint {
	if (typeof value !== "string") throw invalid(name, value);
	const source = value.trim();
	if (source === "none" || source === "transparent") return null;
	if (/^currentcolor$/i.test(source)) return "currentColor";
	if (/^url\(/i.test(source)) throw new Error(`${name}: ${GRADIENT}`);
	try {
		return color(source);
	} catch (error) {
		const message = error instanceof Error ? error.message : String(error);
		throw new Error(
			message.startsWith("a colour with alpha")
				? `${name}: a colour with alpha is not supported; use ${name}Opacity`
				: `${name}: ${message}`,
		);
	}
}

function dashes(value: unknown, name: string): number[] {
	if (value === "none") return [];
	if (typeof value !== "number" && typeof value !== "string") throw invalid(name, value);
	const values = list(String(value), name).map((item) => size(item, name));
	if (values.length > 16) throw new Error(`${name} takes at most 16 values`);
	if (!values.some((item) => item > 0)) return [];
	return values.length % 2 ? [...values, ...values] : values;
}

const SETTERS: Record<string, (state: State, value: unknown, name: string) => void> = {
	fill: (state, value, name) => (state.fill = paint(value, name)),
	stroke: (state, value, name) => (state.stroke = paint(value, name)),
	fillOpacity: (state, value, name) => (state.fillOpacity = unit(value, name)),
	strokeOpacity: (state, value, name) => (state.strokeOpacity = unit(value, name)),
	opacity: (state, value, name) => (state.opacity *= unit(value, name)),
	fillRule: (state, value, name) =>
		(state.evenOdd = keyword(value, name, ["nonzero", "evenodd"]) === 1),
	strokeLinecap: (state, value, name) =>
		(state.cap = keyword(value, name, ["butt", "round", "square"])),
	strokeLinejoin: (state, value, name) =>
		(state.join = keyword(value, name, ["miter", "round", "bevel"])),
	strokeWidth: (state, value, name) => (state.strokeWidth = size(value, name)),
	strokeMiterlimit: (state, value, name) => {
		state.miter = number(value, name);
		if (state.miter < 1) throw invalid(name, value);
	},
	strokeDasharray: (state, value, name) => (state.dashes = dashes(value, name)),
	strokeDashoffset: (state, value, name) => (state.dashOffset = number(value, name)),
};
const PRESENTATION = Object.keys(SETTERS);
const ROOT = ["width", "height", "viewBox", "preserveAspectRatio", "ariaLabel", ...PRESENTATION];

const camel = (name: string) =>
	name.replace(/-([a-z])/g, (_, letter: string) => letter.toUpperCase());

/** React names (`strokeWidth`) and markup names (`stroke-width`) are the same attribute.
 *  `id`, `version`, and namespace declarations change nothing that is drawn. */
function attributes(value: unknown, tag: string, allowed: readonly string[]): Map<string, unknown> {
	const result = new Map<string, unknown>();
	for (const [key, item] of Object.entries(object(value))) {
		if (item === undefined || /^(?:key|ref|children|id|version|xmlns(?::.*)?|xml:space)$/.test(key))
			continue;
		const name = camel(key);
		if (name === "className" || name === "class")
			throw new Error("CSS classes do not apply inside <svg>; use presentation attributes");
		if (!allowed.includes(name)) throw new Error(`unsupported attribute ${key} on <${tag}>`);
		result.set(name, item);
	}
	return result;
}

/** Presentation attributes, then the `style` object or declaration string, which wins. */
function present(state: State, declared: Map<string, unknown>, style: unknown) {
	const values = new Map([...declared].filter(([key]) => Object.hasOwn(SETTERS, key)));
	const entries =
		typeof style === "string"
			? style
					.split(";")
					.filter((item) => item.trim())
					.map((item) => {
						const colon = item.indexOf(":");
						if (colon < 0) throw new Error(`invalid style declaration: ${item.trim()}`);
						return [item.slice(0, colon).trim(), item.slice(colon + 1).trim()] as const;
					})
			: Object.entries(style === undefined ? {} : object(style));
	for (const [key, value] of entries) {
		if (value === undefined) continue;
		if (!Object.hasOwn(SETTERS, camel(key)))
			throw new Error(`unsupported style property inside <svg>: ${key}`);
		values.set(camel(key), value);
	}
	for (const [key, value] of values) if (value !== "inherit") SETTERS[key]!(state, value, key);
}

function multiply(m: Matrix, n: Matrix): Matrix {
	return [
		m[0] * n[0] + m[2] * n[1],
		m[1] * n[0] + m[3] * n[1],
		m[0] * n[2] + m[2] * n[3],
		m[1] * n[2] + m[3] * n[3],
		m[0] * n[4] + m[2] * n[5] + m[4],
		m[1] * n[4] + m[3] * n[5] + m[5],
	];
}

const TRANSFORM = /\s*(matrix|translate|scale|rotate|skewX|skewY)\s*\(([^)]*)\)\s*,?/y;
const ARITY: Record<string, readonly number[]> = {
	matrix: [6],
	translate: [1, 2],
	scale: [1, 2],
	rotate: [1, 3],
	skewX: [1],
	skewY: [1],
};

function transform(value: unknown): Matrix {
	if (typeof value !== "string") throw invalid("transform", value);
	const source = value.trim();
	let result = IDENTITY;
	for (let index = 0; index < source.length; index = TRANSFORM.lastIndex) {
		TRANSFORM.lastIndex = index;
		const match = TRANSFORM.exec(source);
		if (!match)
			throw new Error(
				`invalid transform: ${value} (use matrix, translate, scale, rotate, skewX, or skewY)`,
			);
		const kind = match[1]!;
		const args = list(match[2]!, "transform");
		if (!ARITY[kind]!.includes(args.length))
			throw new Error(`invalid transform: ${kind} takes ${ARITY[kind]!.join(" or ")} values`);
		const [a = 0, b = kind === "scale" ? a : 0, c = 0] = args;
		const cos = Math.cos((a * Math.PI) / 180);
		const sin = Math.sin((a * Math.PI) / 180);
		const step: Matrix =
			kind === "matrix"
				? (args as unknown as Matrix)
				: kind === "translate"
					? [1, 0, 0, 1, a, b]
					: kind === "scale"
						? [a, 0, 0, b, 0, 0]
						: kind === "rotate"
							? [cos, sin, -sin, cos, b - cos * b + sin * c, c - sin * b - cos * c]
							: kind === "skewX"
								? [1, 0, Math.tan((a * Math.PI) / 180), 1, 0, 0]
								: [1, Math.tan((a * Math.PI) / 180), 0, 1, 0, 0];
		result = multiply(result, step);
	}
	return result;
}

class Path {
	readonly ops: number[] = [];
	readonly points: number[] = [];
	x = 0;
	y = 0;
	private startX = 0;
	private startY = 0;
	private open = false;

	constructor(private readonly budget: { segments: number }) {}

	private push(op: number, ...coordinates: number[]) {
		if (--this.budget.segments < 0)
			throw new Error(`<svg> has more than ${MAX_SEGMENTS} path segments; simplify the drawing`);
		this.ops.push(op);
		this.points.push(...coordinates);
	}

	move(x: number, y: number) {
		this.push(0, x, y);
		[this.x, this.y, this.startX, this.startY, this.open] = [x, y, x, y, true];
	}

	/** A segment after a close starts a new subpath at the closed one's start, as in SVG. */
	private reopen() {
		if (!this.open) this.move(this.x, this.y);
	}

	line(x: number, y: number) {
		this.reopen();
		this.push(1, x, y);
		[this.x, this.y] = [x, y];
	}

	cubic(x1: number, y1: number, x2: number, y2: number, x: number, y: number) {
		this.reopen();
		this.push(2, x1, y1, x2, y2, x, y);
		[this.x, this.y] = [x, y];
	}

	close() {
		if (!this.open) return;
		this.push(3);
		[this.x, this.y, this.open] = [this.startX, this.startY, false];
	}
}

/** Endpoint to center parameterisation, then one cubic per quarter turn (SVG 1.1, F.6.5). */
function arc(
	path: Path,
	rx: number,
	ry: number,
	rotation: number,
	large: boolean,
	sweep: boolean,
	x: number,
	y: number,
) {
	const [x1, y1] = [path.x, path.y];
	if (x1 === x && y1 === y) return;
	rx = Math.abs(rx);
	ry = Math.abs(ry);
	if (!rx || !ry) return path.line(x, y);
	const cos = Math.cos((rotation * Math.PI) / 180);
	const sin = Math.sin((rotation * Math.PI) / 180);
	const dx = (x1 - x) / 2;
	const dy = (y1 - y) / 2;
	const px = cos * dx + sin * dy;
	const py = -sin * dx + cos * dy;
	const scale = Math.max(1, Math.sqrt((px * px) / (rx * rx) + (py * py) / (ry * ry)));
	rx *= scale;
	ry *= scale;
	const numerator = rx * rx * ry * ry - rx * rx * py * py - ry * ry * px * px;
	const root =
		(large === sweep ? -1 : 1) *
		Math.sqrt(Math.max(0, numerator / (rx * rx * py * py + ry * ry * px * px)));
	const cx1 = (root * rx * py) / ry;
	const cy1 = (-root * ry * px) / rx;
	const cx = cos * cx1 - sin * cy1 + (x1 + x) / 2;
	const cy = sin * cx1 + cos * cy1 + (y1 + y) / 2;
	const angle = (ux: number, uy: number, vx: number, vy: number) =>
		Math.atan2(ux * vy - uy * vx, ux * vx + uy * vy);
	const start = angle(1, 0, (px - cx1) / rx, (py - cy1) / ry);
	let delta = angle((px - cx1) / rx, (py - cy1) / ry, (-px - cx1) / rx, (-py - cy1) / ry);
	if (!sweep && delta > 0) delta -= 2 * Math.PI;
	else if (sweep && delta < 0) delta += 2 * Math.PI;
	const count = Math.max(1, Math.ceil(Math.abs(delta) / (Math.PI / 2) - 1e-9));
	const step = delta / count;
	const k = (4 / 3) * Math.tan(step / 4);
	const point = (ux: number, uy: number) =>
		[cx + rx * ux * cos - ry * uy * sin, cy + rx * ux * sin + ry * uy * cos] as const;
	for (let index = 0; index < count; index++) {
		const a = start + index * step;
		const b = a + step;
		const [c1x, c1y] = point(Math.cos(a) - k * Math.sin(a), Math.sin(a) + k * Math.cos(a));
		const [c2x, c2y] = point(Math.cos(b) + k * Math.sin(b), Math.sin(b) - k * Math.cos(b));
		const [ex, ey] = index === count - 1 ? [x, y] : point(Math.cos(b), Math.sin(b));
		path.cubic(c1x, c1y, c2x, c2y, ex, ey);
	}
}

const PATH_NUMBER = /[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/y;
const ARGUMENTS: Record<string, number> = {
	m: 2,
	l: 2,
	h: 1,
	v: 1,
	c: 6,
	s: 4,
	q: 4,
	t: 2,
	a: 7,
	z: 0,
};

/** The full `d` grammar, made absolute: H and V become lines, Q, T, S, and A become cubics. */
function pathData(d: string, path: Path) {
	let index = 0;
	const fail = (reason: string): never => {
		throw new Error(`invalid path data at offset ${index}: ${reason}`);
	};
	const skip = () => {
		while (index < d.length && " \t\n\r\f,".includes(d[index]!)) index++;
	};
	const read = () => {
		skip();
		PATH_NUMBER.lastIndex = index;
		const match = PATH_NUMBER.exec(d) ?? fail("expected a number");
		index = PATH_NUMBER.lastIndex;
		const value = Number(match[0]);
		return Number.isFinite(Math.fround(value)) ? value : fail("number out of range");
	};
	const flag = () => {
		skip();
		const value = d[index++];
		if (value !== "0" && value !== "1") fail("expected an arc flag, 0 or 1");
		return value === "1";
	};
	let command = "";
	let previous = "";
	let control = [0, 0];
	for (skip(); index < d.length; skip()) {
		if (/[a-z]/i.test(d[index]!)) {
			if (!(d[index]!.toLowerCase() in ARGUMENTS)) fail(`unknown command ${d[index]}`);
			if (!path.ops.length && !/m/i.test(d[index]!)) fail("path data must start with M");
			command = d[index++]!;
		} else if (!command || /z/i.test(command)) fail("expected a command");
		const lower = command.toLowerCase();
		const relative = command === lower;
		const [x0, y0] = [path.x, path.y];
		const x = () => read() + (relative ? x0 : 0);
		const y = () => read() + (relative ? y0 : 0);
		const reflect = (kinds: readonly string[]) =>
			kinds.includes(previous) ? [2 * x0 - control[0]!, 2 * y0 - control[1]!] : [x0, y0];
		switch (lower) {
			case "m":
				path.move(x(), y());
				// Further pairs after a move are lines.
				command = relative ? "l" : "L";
				break;
			case "l":
				path.line(x(), y());
				break;
			case "h":
				path.line(x(), y0);
				break;
			case "v":
				path.line(x0, y());
				break;
			case "c":
			case "s": {
				const [x1, y1] = lower === "c" ? [x(), y()] : reflect(["c", "s"]);
				control = [x(), y()];
				path.cubic(x1!, y1!, control[0]!, control[1]!, x(), y());
				break;
			}
			case "q":
			case "t": {
				control = lower === "q" ? [x(), y()] : reflect(["q", "t"]);
				const [qx, qy] = control as [number, number];
				const [ex, ey] = [x(), y()];
				path.cubic(
					x0 + (2 / 3) * (qx - x0),
					y0 + (2 / 3) * (qy - y0),
					ex + (2 / 3) * (qx - ex),
					ey + (2 / 3) * (qy - ey),
					ex,
					ey,
				);
				break;
			}
			case "a": {
				const [rx, ry, rotation, large, sweep] = [read(), read(), read(), flag(), flag()];
				arc(path, rx, ry, rotation, large, sweep, x(), y());
				break;
			}
			default:
				path.close();
		}
		previous = lower;
	}
}

const SHAPES: Record<string, (attrs: Map<string, unknown>, path: Path) => void> = {
	rect(attrs, path) {
		const [x, y] = [at(attrs, "x"), at(attrs, "y")];
		const [width, height] = [length(attrs, "width"), length(attrs, "height")];
		const radius = (key: string) => {
			const value = attrs.get(key);
			return value === undefined || value === "auto" ? undefined : size(value, key);
		};
		const [x0, y0] = [radius("rx"), radius("ry")];
		// A missing radius copies the other before each is clamped to half the side.
		const rx = Math.min(x0 ?? y0 ?? 0, width / 2);
		const ry = Math.min(y0 ?? x0 ?? 0, height / 2);
		if (!width || !height) return;
		const [right, bottom] = [x + width, y + height];
		if (!rx || !ry) {
			path.move(x, y);
			path.line(right, y);
			path.line(right, bottom);
			path.line(x, bottom);
			return path.close();
		}
		const [kx, ky] = [rx * KAPPA, ry * KAPPA];
		path.move(x + rx, y);
		path.line(right - rx, y);
		path.cubic(right - rx + kx, y, right, y + ry - ky, right, y + ry);
		path.line(right, bottom - ry);
		path.cubic(right, bottom - ry + ky, right - rx + kx, bottom, right - rx, bottom);
		path.line(x + rx, bottom);
		path.cubic(x + rx - kx, bottom, x, bottom - ry + ky, x, bottom - ry);
		path.line(x, y + ry);
		path.cubic(x, y + ry - ky, x + rx - kx, y, x + rx, y);
		path.close();
	},
	circle: (attrs, path) =>
		ellipse(path, at(attrs, "cx"), at(attrs, "cy"), length(attrs, "r"), length(attrs, "r")),
	ellipse: (attrs, path) =>
		ellipse(path, at(attrs, "cx"), at(attrs, "cy"), length(attrs, "rx"), length(attrs, "ry")),
	line(attrs, path) {
		path.move(at(attrs, "x1"), at(attrs, "y1"));
		path.line(at(attrs, "x2"), at(attrs, "y2"));
	},
	polyline: (attrs, path) => poly(attrs, path),
	polygon(attrs, path) {
		poly(attrs, path);
		path.close();
	},
	path(attrs, path) {
		const d = attrs.get("d") ?? "";
		if (typeof d !== "string") throw invalid("d", d);
		pathData(d, path);
	},
};

const at = (attrs: Map<string, unknown>, key: string) =>
	attrs.has(key) ? number(attrs.get(key), key) : 0;
const length = (attrs: Map<string, unknown>, key: string) =>
	attrs.has(key) ? size(attrs.get(key), key) : 0;

function ellipse(path: Path, cx: number, cy: number, rx: number, ry: number) {
	if (!rx || !ry) return;
	const [kx, ky] = [rx * KAPPA, ry * KAPPA];
	path.move(cx + rx, cy);
	path.cubic(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
	path.cubic(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
	path.cubic(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
	path.cubic(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
	path.close();
}

function poly(attrs: Map<string, unknown>, path: Path) {
	const source = attrs.get("points") ?? "";
	if (typeof source !== "string") throw invalid("points", source);
	const points = list(source, "points");
	if (points.length % 2) throw new Error("points needs an even number of coordinates");
	for (let index = 0; index < points.length; index += 2) {
		const [x, y] = [points[index]!, points[index + 1]!];
		if (index) path.line(x, y);
		else path.move(x, y);
	}
}

type Context = { shapes: Shape[]; elements: number; segments: number };

const located = new WeakSet<Error>();
/** Names the innermost element once, as `locate` does for the host tree. */
function within(error: unknown, where: string) {
	if (error instanceof Error && !located.has(error)) {
		error.message += where;
		located.add(error);
	}
	return error;
}

function text(value: unknown, tag: string): string {
	const parts: string[] = [];
	children(
		value,
		() => {
			throw new Error(`<${tag}> holds only text`);
		},
		parts,
	);
	return parts.join("").replace(/\s+/g, " ").trim();
}

function children(
	value: unknown,
	visit: (element: Record<string, unknown>) => void,
	texts?: string[],
) {
	if (value === null || value === undefined || typeof value === "boolean") return;
	if (Array.isArray(value)) {
		for (const item of value) children(item, visit, texts);
		return;
	}
	if (typeof value === "string" || typeof value === "number") {
		if (texts) texts.push(String(value));
		else if (String(value).trim()) throw new Error(TEXT);
		return;
	}
	visit(object(value));
}

function element(
	value: Record<string, unknown>,
	parent: State,
	context: Context,
	chain: string,
	depth: number,
) {
	if (depth > 64) throw new Error("nesting exceeds 64");
	if (++context.elements > MAX_ELEMENTS)
		throw new Error(`<svg> has more than ${MAX_ELEMENTS} elements`);
	const type = typeof value.type === "string" ? value.type : String(value.type);
	const where = ` on <${type}> in ${chain}`;
	try {
		const input = object(value.props);
		if (type === "title" || type === "desc") {
			attributes(input, type, []);
			text(input.children, type);
			return;
		}
		// Own keys only, so `constructor` is an unknown element like any other.
		const geometry = Object.hasOwn(GEOMETRY, type) ? GEOMETRY[type]! : undefined;
		if (!geometry)
			throw new Error(
				Object.hasOwn(REJECTED, type) ? REJECTED[type] : `unsupported SVG element <${type}>`,
			);
		const attrs = attributes(input, type, [...geometry, ...PRESENTATION, "transform", "style"]);
		const state = { ...parent };
		if (attrs.has("transform"))
			state.matrix = multiply(parent.matrix, transform(attrs.get("transform")));
		present(state, attrs, attrs.get("style"));
		if (type === "g") {
			children(input.children, (child) =>
				element(child, state, context, `<g> > ${chain}`, depth + 1),
			);
			return;
		}
		children(input.children, () => {
			throw new Error(`<${type}> takes no child elements`);
		});
		const path = new Path(context);
		SHAPES[type]!(attrs, path);
		const fill = type === "line" ? null : state.fill;
		const stroke = state.strokeWidth > 0 ? state.stroke : null;
		if (path.ops.length && (fill || stroke))
			context.shapes.push({ ...state, fill, stroke, ops: path.ops, points: path.points });
	} catch (error) {
		throw within(error, where);
	}
}

const ALIGN = /^(?:none|x(Min|Mid|Max)Y(Min|Mid|Max))(?:\s+(meet|slice))?$/;
const FACTOR: Record<string, number> = { Min: 0, Mid: 0.5, Max: 1 };

/** Validates an `<svg>` element and its children into a drawing. Its own `style`,
 *  `className`, and `id` style the box, as for `img`. `chain` names its ancestors. */
export function vector(
	input: Record<string, unknown>,
	chain: string,
	depth: number,
): { drawing: Drawing; alt: string | undefined } {
	const self = chain ? `<svg> > ${chain}` : "<svg>";
	try {
		const own = Object.entries(input).filter(([key]) => key !== "style" && key !== "className");
		const attrs = attributes(Object.fromEntries(own), "svg", ROOT);
		const state = { ...INITIAL };
		present(state, attrs, undefined);
		const box = attrs.has("viewBox") ? list(String(attrs.get("viewBox")), "viewBox") : undefined;
		if (box && (box.length !== 4 || !(box[2]! > 0) || !(box[3]! > 0)))
			throw invalid("viewBox", attrs.get("viewBox"));
		const dimension = (key: string) => {
			if (!attrs.has(key)) return undefined;
			const value = size(attrs.get(key), key);
			if (!value) throw invalid(key, attrs.get(key));
			return value;
		};
		let [width, height] = [dimension("width"), dimension("height")];
		if (box) {
			width ??= height === undefined ? box[2]! : (height * box[2]!) / box[3]!;
			height ??= (width * box[3]!) / box[2]!;
		}
		if (width === undefined || height === undefined)
			throw new Error("an <svg> needs a viewBox, or both width and height");
		const align = ALIGN.exec(String(attrs.get("preserveAspectRatio") ?? "xMidYMid").trim());
		if (!align) throw invalid("preserveAspectRatio", attrs.get("preserveAspectRatio"));
		const context: Context = { shapes: [], elements: 0, segments: MAX_SEGMENTS };
		let title: string | undefined;
		children(input.children, (child) => {
			if (child.type === "title" && title === undefined)
				title = text(object(child.props).children, "title");
			element(child, state, context, self, depth + 1);
		});
		const label = attrs.get("ariaLabel");
		if (label !== undefined && typeof label !== "string") throw invalid("aria-label", label);
		const alt = label ?? title;
		if (alt !== undefined && /\p{Cc}/u.test(alt))
			throw new Error("aria-label and <title> must not hold control characters");
		return {
			alt,
			drawing: {
				// User units are CSS pixels, 0.75 pt each.
				width: width * 0.75,
				height: height * 0.75,
				viewBox: (box ?? [0, 0, width, height]) as [number, number, number, number],
				align: align[1] ? [FACTOR[align[1]]!, FACTOR[align[2]!]!] : undefined,
				slice: align[3] === "slice",
				shapes: context.shapes,
			},
		};
	} catch (error) {
		throw within(error, chain ? ` on <svg> in ${chain}` : " on <svg>");
	}
}

/** The box the viewBox is fitted into; only its aspect ratio reaches the engine. */
/** True for bytes that start like XML text, which PNG and JPEG never do. */
export function isMarkup(bytes: Uint8Array): boolean {
	return /^﻿?\s*</.test(new TextDecoder().decode(bytes.subarray(0, 64)));
}

const TOKEN =
	/<(\/?)([A-Za-z][\w:.-]*)((?:\s+[\w:.-]+\s*=\s*(?:"[^"]*"|'[^']*'))*)\s*(\/?)>|([^<]+)/y;
const ENTITIES: Record<string, string> = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };

function entities(value: string): string {
	return value.replace(/&(?:#(\d+)|#x([\da-f]+)|(\w+));/gi, (_, decimal, hex, name) => {
		const code = decimal ? Number(decimal) : hex ? Number.parseInt(hex, 16) : undefined;
		const result =
			code === undefined
				? Object.hasOwn(ENTITIES, name)
					? ENTITIES[name as string]
					: undefined
				: code <= 0x10ffff
					? String.fromCodePoint(code)
					: undefined;
		if (result === undefined) throw new Error(`unsupported entity in SVG markup: &${name ?? "#"};`);
		return result;
	});
}

type Markup = { type: string; props: { children: (Markup | string)[]; [key: string]: unknown } };

/** SVG file bytes to the element tree JSX would build, so both take the same checks.
 *  No DTD, entities beyond XML's five, CDATA, or processing; comments and `<?xml?>` are skipped. */
export function markup(bytes: Uint8Array): Markup {
	if (bytes.byteLength > MAX_MARKUP) throw new Error("SVG markup exceeds 4 MiB");
	let source: string;
	try {
		source = new TextDecoder("utf-8", { fatal: true }).decode(bytes).replace(/^﻿/, "");
	} catch {
		throw new Error("SVG markup must be UTF-8");
	}
	const root: Markup = { type: "", props: { children: [] } };
	const stack = [root];
	for (let index = 0; index < source.length;) {
		const terminator = source.startsWith("<!--", index)
			? "-->"
			: source.startsWith("<?", index)
				? "?>"
				: source.startsWith("<!DOCTYPE", index)
					? ">"
					: undefined;
		if (terminator) {
			const start = index + (terminator === "-->" ? 4 : terminator === "?>" ? 2 : 9);
			const end = source.indexOf(terminator, start);
			if (
				end < 0 ||
				(terminator === "-->" && source.indexOf("--", start) !== end) ||
				(terminator === ">" && source.slice(start, end).includes("["))
			)
				throw new Error(`unsupported SVG markup at offset ${index}`);
			index = end + terminator.length;
			continue;
		}
		TOKEN.lastIndex = index;
		const match = TOKEN.exec(source);
		if (!match)
			throw new Error(
				`unsupported SVG markup at offset ${index}: ${JSON.stringify(source.slice(index, index + 24))}`,
			);
		index = TOKEN.lastIndex;
		const [, closing, name, attributes, selfClosing, content] = match;
		const parent = stack.at(-1)!;
		if (content !== undefined) parent.props.children.push(entities(content));
		else if (closing) {
			if (parent.type !== name || attributes || selfClosing)
				throw new Error(`SVG markup closes <${name}> where <${parent.type}> is open`);
			stack.pop();
		} else {
			const element: Markup = { type: name!, props: { children: [] } };
			for (let offset = 0; offset < attributes!.length;) {
				const equal = attributes!.indexOf("=", offset);
				if (equal < 0) throw new Error("invalid SVG attribute");
				const key = attributes!.slice(offset, equal).trim();
				let quote = equal + 1;
				while (attributes![quote]?.trim() === "") quote++;
				const delimiter = attributes![quote];
				if (delimiter !== '"' && delimiter !== "'") throw new Error("invalid SVG attribute");
				const end = attributes!.indexOf(delimiter, quote + 1);
				if (end < 0) throw new Error("invalid SVG attribute");
				if (key in element.props) throw new Error(`SVG markup repeats attribute ${key}`);
				element.props[key] = entities(attributes!.slice(quote + 1, end));
				offset = end + 1;
			}
			parent.props.children.push(element);
			if (!selfClosing) {
				if (stack.length > 64) throw new Error("nesting exceeds 64");
				stack.push(element);
			}
		}
	}
	if (stack.length > 1) throw new Error(`SVG markup leaves <${stack.at(-1)!.type}> open`);
	const [svg, ...others] = root.props.children.filter(
		(child) => typeof child !== "string" || child.trim(),
	);
	if (typeof svg !== "object" || svg.type !== "svg" || others.length)
		throw new Error("SVG markup must hold one <svg> root element");
	if ("style" in svg.props || "class" in svg.props)
		throw new Error("style and class on the SVG root are not supported; style the <img>");
	return svg;
}
