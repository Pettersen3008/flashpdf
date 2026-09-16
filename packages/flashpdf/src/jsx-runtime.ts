export type Child = Element | string | number | boolean | null | undefined | readonly Child[];
export type Style = {
  display?: "block" | "flex";
  flexDirection?: "row" | "column";
  flex?: number;
  width?: number | `${number}${"pt" | "px" | "%"}`;
  minWidth?: number | `${number}${"pt" | "px" | "%"}`;
  maxWidth?: number | `${number}${"pt" | "px" | "%"}`;
  height?: number | `${number}${"pt" | "px" | "%"}`;
  margin?: number | string;
  marginTop?: number | string;
  marginRight?: number | string;
  marginBottom?: number | string;
  marginLeft?: number | string;
  padding?: number | string;
  paddingTop?: number | string;
  paddingRight?: number | string;
  paddingBottom?: number | string;
  paddingLeft?: number | string;
  border?: string | number;
  borderWidth?: number | string;
  borderColor?: string;
  borderRadius?: number | string;
  background?: string;
  backgroundColor?: string;
  gap?: number | string;
  fontSize?: number | string;
  fontFamily?: string;
  fontWeight?: number | "normal" | "bold";
  lineHeight?: number | string;
  letterSpacing?: number | string;
  textAlign?: "left" | "center" | "right";
  textDecoration?: "none" | "underline";
  color?: string;
  breakBefore?: "auto" | "page";
  breakAfter?: "auto" | "page";
  breakInside?: "auto" | "avoid";
  [customProperty: `--${string}`]: string | number | undefined;
};
type Children = { children?: Child };
type Key = { key?: string | number };
export type HtmlProps = Children &
  Key & { className?: string; id?: string; style?: Style | string };
export type Element = { readonly type: string | typeof Fragment; readonly props: unknown };
export const Fragment = Symbol("FlashPDF.Fragment");
export function jsx(
  type: string | typeof Fragment | ((props: never) => Element),
  props: unknown,
): Element {
  // The compiler checks component props at the JSX call site; lowering validates the result.
  return typeof type === "function" ? type(props as never) : { type, props };
}
export const jsxs = jsx;
export namespace JSX {
  export type Element = import("./jsx-runtime.js").Element;
  export interface ElementChildrenAttribute {
    children: unknown;
  }
  /** Lets `key` sit on a component call, not just an intrinsic tag. */
  export interface IntrinsicAttributes extends Key {}
  export interface IntrinsicElements {
    div: HtmlProps;
    main: HtmlProps;
    section: HtmlProps;
    article: HtmlProps;
    header: HtmlProps;
    footer: HtmlProps;
    span: HtmlProps;
    p: HtmlProps;
    h1: HtmlProps;
    h2: HtmlProps;
    h3: HtmlProps;
    h4: HtmlProps;
    h5: HtmlProps;
    h6: HtmlProps;
  }
}
