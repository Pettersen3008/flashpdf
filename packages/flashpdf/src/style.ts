import { object, number } from "./assert.js";
import { supported, unsupported } from "./css.js";

export function style(value: unknown, tag = ""): Record<string, unknown> {
  if (value === undefined) return {};
  const where = tag && ` on <${tag}>`;
  const result = object(value);
  for (const key of Object.keys(result)) {
    if (key.startsWith("--")) continue;
    const reason = unsupported[key as keyof typeof unsupported];
    if (reason) throw new Error(`unsupported style property: ${key} (${reason})${where}`);
    if (!supported.has(key as never)) throw new Error(`unsupported style property: ${key}${where}`);
  }
  if (result.display !== undefined && result.display !== "block" && result.display !== "flex")
    throw new Error("invalid display");
  if (
    result.flexDirection !== undefined &&
    result.flexDirection !== "row" &&
    result.flexDirection !== "column"
  )
    throw new Error("invalid flex direction");
  if (
    result.textAlign !== undefined &&
    result.textAlign !== "left" &&
    result.textAlign !== "center" &&
    result.textAlign !== "right"
  )
    throw new Error("invalid text alignment");
  for (const key of ["breakBefore", "breakAfter"] as const)
    if (result[key] !== undefined && result[key] !== "auto" && result[key] !== "page")
      throw new Error(`invalid ${key}`);
  if (
    result.breakInside !== undefined &&
    result.breakInside !== "auto" &&
    result.breakInside !== "avoid"
  )
    throw new Error("invalid breakInside");
  if (result.flex !== undefined) number(result.flex, true);
  if (result.gap !== undefined) point(result.gap);
  if (result.fontSize !== undefined) point(result.fontSize, 12, true);
  const family = result.fontFamily;
  if (
    family !== undefined &&
    (typeof family !== "string" || !family.trim() || family.includes(","))
  )
    throw new Error("fontFamily must name one registered family");
  if (result.width !== undefined) {
    if (typeof result.width === "number") number(result.width, true);
    else if (
      typeof result.width !== "string" ||
      !/^(?:\d+(?:\.\d*)?|\.\d+)(?:pt|px|%)$/.test(result.width)
    )
      throw new Error("invalid width");
  }
  return result;
}

export function point(value: unknown, parent = 12, positive = false): number {
  if (typeof value === "number") return number(value, positive);
  if (value === "0") return 0;
  if (typeof value !== "string") throw new Error("invalid length");
  const match = /^(\d+(?:\.\d*)?|\.\d+)(pt|px|em|rem)$/.exec(value.trim());
  if (!match) throw new Error("invalid length");
  const scale = match[2] === "px" ? 0.75 : match[2] === "em" ? parent : match[2] === "rem" ? 12 : 1;
  return number(Number(match[1]) * scale, positive);
}

export type Box = {
  margin: [number, number, number, number];
  padding: [number, number, number, number];
  border: number;
  background?: [number, number, number];
  borderColor: [number, number, number];
};

export function edges(
  s: Record<string, unknown>,
  name: "margin" | "padding",
  parent: number,
): [number, number, number, number] {
  const shorthand = s[name];
  let values: number[] = [];
  if (shorthand !== undefined) {
    values = (typeof shorthand === "string" ? shorthand.trim().split(/\s+/) : [shorthand]).map(
      (value) => point(value, parent),
    );
    if (values.length < 1 || values.length > 4) throw new Error(`invalid ${name}`);
  }
  const expanded =
    values.length === 1
      ? [values[0], values[0], values[0], values[0]]
      : values.length === 2
        ? [values[0], values[1], values[0], values[1]]
        : values.length === 3
          ? [values[0], values[1], values[2], values[1]]
          : values.length === 4
            ? values
            : [0, 0, 0, 0];
  return ["Top", "Right", "Bottom", "Left"].map((side, index) =>
    s[`${name}${side}`] === undefined ? expanded[index] : point(s[`${name}${side}`], parent),
  ) as [number, number, number, number];
}

export function boxStyle(s: Record<string, unknown>, parent: number): Box | undefined {
  const hasBox = [
    "margin",
    "marginTop",
    "marginRight",
    "marginBottom",
    "marginLeft",
    "padding",
    "paddingTop",
    "paddingRight",
    "paddingBottom",
    "paddingLeft",
    "border",
    "borderWidth",
    "borderColor",
    "background",
    "backgroundColor",
  ].some((key) => s[key] !== undefined);
  if (!hasBox) return undefined;
  let border = s.borderWidth === undefined ? 0 : point(s.borderWidth, parent);
  let borderColor: [number, number, number] =
    s.borderColor === undefined ? [0, 0, 0] : color(s.borderColor);
  if (s.border !== undefined) {
    if (typeof s.border === "string" && /\s/.test(s.border)) {
      const tokens = s.border.trim().split(/\s+/);
      border = point(tokens[0], parent);
      const paint = tokens.find(
        (token) => token.startsWith("#") || /^(?:black|white|red|green|blue)$/.test(token),
      );
      if (paint) borderColor = color(paint);
    } else border = point(s.border, parent);
  }
  const backgroundValue = s.backgroundColor ?? s.background;
  return {
    margin: edges(s, "margin", parent),
    padding: edges(s, "padding", parent),
    border,
    background: backgroundValue === undefined ? undefined : color(backgroundValue),
    borderColor,
  };
}

export function color(value: unknown): [number, number, number] {
  if (typeof value !== "string") throw new Error("invalid color");
  const hex = value.trim().toLowerCase();
  const named: Record<string, string> = {
    black: "#000000",
    white: "#ffffff",
    red: "#ff0000",
    green: "#008000",
    blue: "#0000ff",
    transparent: "#000000",
  };
  const source = named[hex] ?? hex;
  const match = /^#([\da-f]{3}|[\da-f]{6})$/.exec(source);
  if (!match) throw new Error("unsupported color");
  const value6 =
    match[1].length === 3 ? [...match[1]].map((part) => part + part).join("") : match[1];
  return [
    Number.parseInt(value6.slice(0, 2), 16),
    Number.parseInt(value6.slice(2, 4), 16),
    Number.parseInt(value6.slice(4, 6), 16),
  ];
}

export function bold(value: unknown): boolean {
  return (
    value === "bold" ||
    (typeof value === "number" && value >= 600) ||
    (typeof value === "string" && Number(value) >= 600)
  );
}

export type FontSlots = { regular: number; bold?: number };
export const helvetica: FontSlots = { regular: 0, bold: 1 };

export function font(
  s: Record<string, unknown>,
  inherited: FontSlots,
  inheritedBold: boolean,
  fonts: ReadonlyMap<string, FontSlots>,
) {
  const requested = s.fontFamily;
  const family =
    requested === undefined
      ? inherited
      : typeof requested === "string"
        ? fonts.get(requested.trim())
        : undefined;
  if (!family) throw new Error(`unregistered font family: ${requested}`);
  const weight = s.fontWeight === undefined ? inheritedBold : bold(s.fontWeight);
  if (weight && family.bold === undefined)
    throw new Error(`font family has no bold face: ${s.fontFamily ?? "inherited"}`);
  return { family, weight };
}

export function nativeColumn(value: unknown): { kind: number; value: number } {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    return { kind: 1, value: 1 };
  const node = object(value);
  const p = object(node.props);
  const s = style(p.style);
  if (typeof s.flex === "number") return { kind: 1, value: number(s.flex, true) };
  if (typeof s.width === "number") return { kind: 0, value: number(s.width, true) };
  if (typeof s.width === "string") {
    const value = number(Number.parseFloat(s.width), true);
    return s.width.endsWith("pt")
      ? { kind: 0, value }
      : s.width.endsWith("px")
        ? { kind: 0, value: value * 0.75 }
        : { kind: 1, value };
  }
  return { kind: 1, value: 1 };
}
