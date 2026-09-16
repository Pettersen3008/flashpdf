import type { Element, Style } from "./jsx-runtime.js";

type Identity = { tag: string; id?: string; classes: readonly string[] };
type Rule = {
  selector: string;
  declarations: Record<string, string>;
  order: number;
  specificity: number;
};
/**
 * The UA sheet's heading defaults. They sit between the inherited values and any
 * matching rule, so `main { font-size: 11pt }` scales a heading instead of
 * flattening it, which is what a browser does.
 */
const uaStyles: Record<string, Record<string, string>> = {
  h1: { fontSize: "2em", fontWeight: "bold" },
  h2: { fontSize: "1.5em", fontWeight: "bold" },
  h3: { fontSize: "1.17em", fontWeight: "bold" },
  h4: { fontSize: "1em", fontWeight: "bold" },
  h5: { fontSize: "0.83em", fontWeight: "bold" },
  h6: { fontSize: "0.67em", fontWeight: "bold" },
};
const inherited = new Set([
  "color",
  "fontSize",
  "fontFamily",
  "fontWeight",
  "lineHeight",
  "letterSpacing",
  "textAlign",
  "textDecoration",
]);
const cssNames: Record<string, keyof Style> = {
  "background-color": "backgroundColor",
  background: "background",
  "border-color": "borderColor",
  "border-radius": "borderRadius",
  "border-width": "borderWidth",
  "break-after": "breakAfter",
  "break-before": "breakBefore",
  "break-inside": "breakInside",
  "flex-direction": "flexDirection",
  "font-family": "fontFamily",
  "font-size": "fontSize",
  "font-weight": "fontWeight",
  "letter-spacing": "letterSpacing",
  "line-height": "lineHeight",
  "max-width": "maxWidth",
  "min-width": "minWidth",
  "margin-bottom": "marginBottom",
  "margin-left": "marginLeft",
  "margin-right": "marginRight",
  "margin-top": "marginTop",
  "padding-bottom": "paddingBottom",
  "padding-left": "paddingLeft",
  "padding-right": "paddingRight",
  "padding-top": "paddingTop",
  "text-align": "textAlign",
  "text-decoration": "textDecoration",
};
/** Properties the renderer honours. `STYLE_KEYS` in the lowering mirrors this. */
export const supported = new Set<keyof Style>([
  "display",
  "flexDirection",
  "flex",
  "width",
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
  "gap",
  "fontSize",
  "fontFamily",
  "fontWeight",
  "textAlign",
  "color",
  "breakBefore",
  "breakAfter",
  "breakInside",
]);

/**
 * CSS the parser recognises but the renderer cannot honour. Naming the reason
 * here keeps every rejection identical whether it arrives from a stylesheet, an
 * inline style object or a stylesheet compiled by the consumer's build tool.
 */
export const unsupported: Partial<Record<keyof Style, string>> = {
  lineHeight: "line height is fixed to the font size",
  letterSpacing: "text shaping is not implemented",
  textDecoration: "text decoration is not implemented",
  borderRadius: "box corners are square",
  minWidth: "only width on a flex row child is implemented",
  maxWidth: "only width on a flex row child is implemented",
  height: "block height comes from content",
};

export function property(name: string, context = ""): keyof Style | `--${string}` {
  if (name.startsWith("--")) return name as `--${string}`;
  const result = cssNames[name] ?? (name as keyof Style);
  const reason = unsupported[result];
  if (reason) throw new Error(`unsupported CSS property: ${name} (${reason})${context}`);
  if (!supported.has(result)) throw new Error(`unsupported CSS property: ${name}${context}`);
  return result;
}
function declarations(source: string, context = ""): Record<string, string> {
  const result: Record<string, string> = {};
  for (const item of source.split(";")) {
    if (!item.trim()) continue;
    const colon = item.indexOf(":");
    if (colon < 1) throw new Error(`invalid CSS declaration: ${item.trim()}${context}`);
    result[property(item.slice(0, colon).trim(), context)] = item.slice(colon + 1).trim();
  }
  return result;
}
function specificity(selector: string) {
  return (
    (selector.match(/#[\w-]+/g)?.length ?? 0) * 100 +
    (selector.match(/\.[\w-]+/g)?.length ?? 0) * 10 +
    (selector.match(/(^|[ >])[a-z][\w-]*/gi)?.length ?? 0)
  );
}
function matchesPart(part: string, node: Identity) {
  const tag = part.match(/^[a-z][\w-]*/i)?.[0];
  return (
    (!tag || tag === node.tag) &&
    !part.match(/#[\w-]+/g)?.some((value) => value.slice(1) !== node.id) &&
    !part.match(/\.[\w-]+/g)?.some((value) => !node.classes.includes(value.slice(1)))
  );
}
function matches(selector: string, node: Identity, ancestors: readonly Identity[]) {
  const pieces = selector
    .trim()
    .replace(/\s*>\s*/g, ">")
    .split(/(>|\s+)/)
    .filter(Boolean);
  let current: Identity | undefined = node;
  let ancestor = ancestors.length - 1;
  for (let index = pieces.length - 1; index >= 0;) {
    const part = pieces[index--];
    if (part === ">") {
      current = ancestors[ancestor--];
      if (!current || !matchesPart(pieces[index--], current)) return false;
      continue;
    }
    if (!current || !matchesPart(part, current)) return false;
    if (index < 0) return true;
    if (pieces[index] === " ") index--;
    const wanted = pieces[index--];
    while (ancestor >= 0 && !matchesPart(wanted, ancestors[ancestor])) ancestor--;
    if (ancestor < 0) return false;
    current = ancestors[ancestor--];
  }
  return true;
}
function resolveVariable(value: unknown, variables: Record<string, unknown>, depth = 0): unknown {
  if (typeof value !== "string" || depth > 16) return value;
  return value.replace(
    /var\((--[\w-]+)(?:,\s*([^()]+))?\)/g,
    (_, name: string, fallback: string | undefined) =>
      String(resolveVariable(variables[name] ?? fallback ?? "", variables, depth + 1)),
  );
}
export function stylesheet(source: string): string {
  parse(source);
  return source;
}
function position(source: string, index: number) {
  const before = source.slice(0, index);
  return `${before.split("\n").length}:${before.length - before.lastIndexOf("\n")}`;
}
function parse(source: string): Rule[] {
  const rules: Rule[] = [];
  const clean = source.replace(/\/\*[\s\S]*?\*\//g, "");
  const at = clean.indexOf("@");
  if (at >= 0)
    throw new Error(
      `unsupported CSS at-rule: ${clean.slice(at).split(/[\s{;]/)[0]} at ${position(clean, at)}`,
    );
  const blocks = clean.matchAll(/([^{}]+)\{([^{}]*)\}/g);
  let end = 0;
  for (const block of blocks) {
    end = block.index! + block[0].length;
    const at = ` at ${position(clean, block.index!)}`;
    for (const selector of block[1].split(",").map((value) => value.trim())) {
      if (!selector || !/^[\w.#\s>-]+$/.test(selector))
        throw new Error(`unsupported CSS selector: ${selector || block[1].trim()}${at}`);
      rules.push({
        selector,
        declarations: declarations(block[2], ` in selector ${JSON.stringify(selector)}${at}`),
        order: rules.length,
        specificity: specificity(selector),
      });
    }
  }
  if (clean.slice(end).trim()) throw new Error(`invalid CSS stylesheet at ${position(clean, end)}`);
  return rules;
}
function inline(value: Style | string | undefined): Record<string, unknown> {
  if (value === undefined) return {};
  return typeof value === "string" ? declarations(value, " in inline style") : value;
}
function node(value: unknown): value is Element {
  return typeof value === "object" && value !== null && "type" in value && "props" in value;
}
function visit(
  value: unknown,
  rules: Rule[],
  ancestors: readonly Identity[],
  parent: Record<string, unknown>,
): unknown {
  if (Array.isArray(value)) return value.map((child) => visit(child, rules, ancestors, parent));
  if (
    !node(value) ||
    typeof value.type !== "string" ||
    ![
      "div",
      "main",
      "section",
      "article",
      "header",
      "footer",
      "span",
      "p",
      "h1",
      "h2",
      "h3",
      "h4",
      "h5",
      "h6",
    ].includes(value.type)
  )
    return value;
  const props = value.props as Record<string, unknown>;
  const identity: Identity = {
    tag: value.type,
    id: typeof props.id === "string" ? props.id : undefined,
    classes:
      typeof props.className === "string" ? props.className.split(/\s+/).filter(Boolean) : [],
  };
  const result: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(parent))
    if (key.startsWith("--") || inherited.has(key)) result[key] = item;
  Object.assign(result, uaStyles[value.type]);
  for (const rule of rules
    .filter((rule) => matches(rule.selector, identity, ancestors))
    .sort((a, b) => a.specificity - b.specificity || a.order - b.order))
    Object.assign(result, rule.declarations);
  Object.assign(result, inline(props.style as Style | string | undefined));
  for (const [key, item] of Object.entries(result)) result[key] = resolveVariable(item, result);
  const { className: _className, id: _id, style: _style, children, ...rest } = props;
  return {
    type: value.type,
    props: {
      ...rest,
      style: result,
      children: visit(children, rules, [...ancestors, identity], result),
    },
  };
}
export function resolveStyles(value: unknown, sheets: readonly string[] = []): unknown {
  // `parse` numbers rules per sheet, so reindex: a later sheet outranks an
  // earlier one at equal specificity, exactly as a browser stacks <link> tags.
  const rules = sheets.flatMap(parse).map((rule, order) => ({ ...rule, order }));
  return visit(value, rules, [], {});
}
