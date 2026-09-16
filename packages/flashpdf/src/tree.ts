import { object, props, text } from "./assert.js";
import { Fragment } from "./jsx-runtime.js";

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
  } else if (typeof value === "object" && object(value).type === Fragment) {
    yield* children(props(object(value).props, ["children"]).children, depth + 1);
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
