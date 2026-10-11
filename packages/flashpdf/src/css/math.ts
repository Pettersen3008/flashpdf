import { point } from "../style/length.js";

type Quantity = { value: number; kind: "number" | "length" | "percent" };

const MAX_EXPRESSION = 1024;
const MAX_DEPTH = 16;
/** Properties whose parser accepts a percentage; the engine resolves it against the container later. */
const PERCENT = new Set(["width", "height", "lineHeight"]);
const START = /(?<![\w-])(?:calc|min|max|clamp)\(/iy;
// A number takes its sign before `+` and `-` match, since CSS requires whitespace around those operators.
const TOKEN =
	/\s*(?:([+-]?(?:\d+\.?\d*|\.\d+))([a-z]+|%)?(?![\w.-])|(?:calc|min|max|clamp)?\(|[*/+,)-])/iy;

function lex(expression: string): string[] {
	const tokens: string[] = [];
	let end = 0;
	TOKEN.lastIndex = 0;
	for (let match = TOKEN.exec(expression); match; match = TOKEN.exec(expression)) {
		tokens.push(match[0].trim().toLowerCase());
		end = TOKEN.lastIndex;
	}
	const rest = expression.slice(end).trim();
	if (rest) throw new Error(`unexpected "${rest}"`);
	return tokens;
}

function quantity(token: string, em: number): Quantity {
	const [, sign, digits, unit] = /^([+-]?)(\d+\.?\d*|\.\d+)(.*)$/.exec(token)!;
	const factor = sign === "-" ? -1 : 1;
	if (unit === "") return { kind: "number", value: factor * Number(digits) };
	if (unit === "%") return { kind: "percent", value: factor * Number(digits) };
	return { kind: "length", value: factor * point(`${digits}${unit}`, em) };
}

/** `+`, `-`, `min()`, `max()`, and `clamp()` take operands of one kind. */
function same(left: Quantity, right: Quantity, verb: string) {
	if (left.kind === right.kind) return;
	throw new Error(
		left.kind === "number" || right.kind === "number"
			? `cannot ${verb} a number and a length or percentage`
			: "a percentage cannot combine with a length, because the engine resolves percentages when it lays the box out",
	);
}

function evaluate(expression: string, em: number): Quantity {
	const tokens = lex(expression);
	let at = 0;
	const sum = (depth: number): Quantity => {
		let left = product(depth);
		while (tokens[at] === "+" || tokens[at] === "-") {
			const add = tokens[at++] === "+";
			const right = product(depth);
			same(left, right, add ? "add" : "subtract");
			left = { kind: left.kind, value: add ? left.value + right.value : left.value - right.value };
		}
		return left;
	};
	const product = (depth: number): Quantity => {
		let left = operand(depth);
		while (tokens[at] === "*" || tokens[at] === "/") {
			const multiply = tokens[at++] === "*";
			const right = operand(depth);
			if (!multiply) {
				if (right.kind !== "number") throw new Error("the divisor must be a number");
				if (right.value === 0) throw new Error("division by zero");
				left = { kind: left.kind, value: left.value / right.value };
			} else if (left.kind !== "number" && right.kind !== "number") {
				throw new Error("cannot multiply two lengths or percentages");
			} else {
				const kind = left.kind === "number" ? right.kind : left.kind;
				left = { kind, value: left.value * right.value };
			}
		}
		return left;
	};
	const operand = (depth: number): Quantity => {
		const token = tokens[at++];
		if (token === undefined) throw new Error("unexpected end");
		if (depth > MAX_DEPTH) throw new Error("nested too deeply");
		if (!token.endsWith("(")) {
			if (/^[+-]?[\d.]/.test(token)) return quantity(token, em);
			throw new Error(`unexpected "${token}"`);
		}
		const name = token.slice(0, -1);
		const args = [sum(depth + 1)];
		while (name && tokens[at] === ",") {
			at++;
			args.push(sum(depth + 1));
		}
		if (tokens[at] !== ")")
			throw new Error(tokens[at] === undefined ? 'expected ")"' : `unexpected "${tokens[at]}"`);
		at++;
		if (!name || name === "calc") {
			if (args.length > 1) throw new Error("calc() takes one expression");
			return args[0]!;
		}
		if (name === "clamp" && args.length !== 3) throw new Error("clamp() takes 3 values");
		for (const arg of args) same(args[0]!, arg, "compare");
		const values = args.map((arg) => arg.value);
		const value =
			name === "min"
				? Math.min(...values)
				: name === "max"
					? Math.max(...values)
					: Math.max(values[0]!, Math.min(values[1]!, values[2]!));
		return { kind: args[0]!.kind, value };
	};
	const result = sum(0);
	if (at < tokens.length) throw new Error(`unexpected "${tokens[at]}"`);
	return result;
}

function format(result: Quantity, property: string): string {
	if (!(Math.abs(result.value) < 1e9)) throw new Error("the result is out of range");
	if (result.kind === "percent" && !PERCENT.has(property))
		throw new Error(`${property} does not take a percentage`);
	const text = String(Number(result.value.toFixed(6)));
	return result.kind === "length" ? `${text}pt` : result.kind === "percent" ? `${text}%` : text;
}

/** Index of the `)` closing the `(` at `open`, searching at most MAX_EXPRESSION characters. */
function closing(value: string, open: number): number {
	let depth = 0;
	for (let index = open; index < Math.min(value.length, open + MAX_EXPRESSION); index++) {
		if (value[index] === "(") depth++;
		else if (value[index] === ")" && --depth === 0) return index;
	}
	throw new Error(`unbalanced parentheses or longer than ${MAX_EXPRESSION} characters`);
}

/** Replaces each top-level `calc()`, `min()`, `max()`, and `clamp()` in `value` with the plain
 *  number, `pt` length, or percentage it computes to, so every later parser sees plain values.
 *  `em` is the font size an `em` unit resolves against. */
export function evaluateMath(property: string, value: string, em: number): string {
	if (!/(?:calc|min|max|clamp)\(/i.test(value)) return value;
	let result = "";
	let copied = 0;
	let depth = 0;
	for (let index = 0; index < value.length; index++) {
		const character = value[index]!;
		START.lastIndex = index;
		if (character === '"' || character === "'") {
			index = value.indexOf(character, index + 1);
			if (index < 0) break;
		} else if (depth === 0 && START.test(value)) {
			const open = START.lastIndex - 1;
			let end = open;
			try {
				end = closing(value, open);
				result +=
					value.slice(copied, index) + format(evaluate(value.slice(index, end + 1), em), property);
			} catch (error) {
				throw new Error(
					`invalid ${value.slice(index, end + 1)} in ${property}: ${(error as Error).message}`,
				);
			}
			copied = end + 1;
			index = end;
		} else if (character === "(") depth++;
		else if (character === ")") depth--;
	}
	return result + value.slice(copied);
}
