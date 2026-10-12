export type Length = number | "0" | `${number}${"pt" | "px" | "em" | "rem"}`;
export type Width = number | `${number}${"pt" | "px" | "%"}`;
export type SignedLength = Length | `-${Length}`;
type TextDecorationLine = "none" | "underline" | "line-through" | "underline line-through";
export type ListStyleType =
	| "disc"
	| "circle"
	| "square"
	| "decimal"
	| "decimal-leading-zero"
	| "lower-alpha"
	| "lower-latin"
	| "upper-alpha"
	| "upper-latin"
	| "lower-roman"
	| "upper-roman"
	| "none";
export type Edges =
	| Length
	| `${Length} ${Length}`
	| `${Length} ${Length} ${Length}`
	| `${Length} ${Length} ${Length} ${Length}`;

export type BorderStyle = "none" | "hidden" | "solid" | "dashed" | "dotted";

export type Style = {
	display?: "block" | "flex";
	flexDirection?: "row" | "column";
	flex?: number;
	width?: Width;
	/** Only `img` takes a height; block height comes from content. */
	height?: Width;
	margin?: Edges;
	marginTop?: Length;
	marginRight?: Length;
	marginBottom?: Length;
	marginLeft?: Length;
	padding?: Edges;
	paddingTop?: Length;
	paddingRight?: Length;
	paddingBottom?: Length;
	paddingLeft?: Length;
	border?: string | number;
	borderWidth?: Length;
	borderColor?: string;
	borderTop?: string | number;
	borderRight?: string | number;
	borderBottom?: string | number;
	borderLeft?: string | number;
	/** One to four of `none`, `solid`, `dashed`, `dotted`, top first. */
	borderStyle?: string;
	borderTopStyle?: BorderStyle;
	borderRightStyle?: BorderStyle;
	borderBottomStyle?: BorderStyle;
	borderLeftStyle?: BorderStyle;
	/** One to four lengths or percents, then optionally `/` and one to four vertical ones. */
	borderRadius?: string | number;
	/** One or two lengths or percents: horizontal, then vertical. */
	borderTopLeftRadius?: string | number;
	borderTopRightRadius?: string | number;
	borderBottomRightRadius?: string | number;
	borderBottomLeftRadius?: string | number;
	/** A number from 0 to 1, or a percent; it multiplies down the tree. */
	opacity?: number | string;
	background?: string;
	backgroundColor?: string;
	gap?: Length;
	fontSize?: Length;
	/** A unitless number is a factor of the font size; `%` and `em` resolve against this element's size. */
	lineHeight?: number | "normal" | `${number}` | `${number}%` | Length;
	fontFamily?: string;
	fontWeight?: number | "normal" | "bold";
	/** `auto` and `normal` kern letter pairs within a word. */
	fontKerning?: "auto" | "normal" | "none";
	/** `oblique` uses the italic face too; a family without one is slanted. */
	fontStyle?: "normal" | "italic" | "oblique";
	/** Added after every character; negative values tighten. */
	letterSpacing?: "normal" | SignedLength;
	/** Added after every space and no-break space. */
	wordSpacing?: "normal" | SignedLength;
	textAlign?: "left" | "center" | "right" | "justify";
	textTransform?: "none" | "uppercase" | "lowercase" | "capitalize";
	/** Lines, optionally followed by a colour, as in `underline red`. */
	textDecoration?: TextDecorationLine | `${TextDecorationLine} ${string}`;
	textDecorationColor?: string;
	/** Collapses whitespace unless `pre`, `pre-wrap`, or `pre-line`. `pre` and `nowrap` reject a line wider than its container. */
	whiteSpace?: "normal" | "nowrap" | "pre" | "pre-wrap" | "pre-line";
	/** Only on inline elements: `sub` and `super` shift the baseline by a fraction of the parent's size. */
	verticalAlign?: "baseline" | "sub" | "super";
	/** Omitted parts reset to `disc` and `outside`, as in CSS. */
	listStyle?: string;
	listStyleType?: ListStyleType;
	listStylePosition?: "outside" | "inside";
	color?: string;
	breakBefore?: "auto" | "page";
	breakAfter?: "auto" | "page";
	breakInside?: "auto" | "avoid";
	[customProperty: `--${string}`]: string | number | undefined;
};

export type StyleInput = { [Property in keyof Style]: Style[Property] | undefined };

export type Element = { readonly type: unknown; readonly props: unknown };
export type PdfNode =
	| Element
	| string
	| number
	| bigint
	| boolean
	| null
	| undefined
	| readonly PdfNode[];
export type CommonProps = {
	key?: string | number | undefined;
	id?: string | undefined;
	className?: string | undefined;
	style?: StyleInput | undefined;
};
export type BlockProps = CommonProps & { children?: PdfNode | undefined };
export type VoidProps = CommonProps & { children?: never };
/** `start` is the first item's number, and `reversed` counts down. */
export type OrderedListProps = BlockProps & {
	start?: number | undefined;
	reversed?: boolean | undefined;
};
/** `value` sets this item's number in an `ol`. */
export type ListItemProps = BlockProps & { value?: number | undefined };
/** `src` is the PNG, JPEG, or SVG file bytes; `alt` is stored for accessibility tagging. */
export type ImageProps = VoidProps & { src: Uint8Array; alt?: string | undefined };
/** `href` must be an absolute `http:`, `https:`, or `mailto:` URL, or `#id` for the element with that id. */
export type LinkProps = BlockProps & { href: string };

type SvgNumber = number | string;
/** SVG presentation attributes, by their React names. */
export type SvgPresentation = {
	fill?: string | undefined;
	fillOpacity?: SvgNumber | undefined;
	fillRule?: "nonzero" | "evenodd" | undefined;
	stroke?: string | undefined;
	strokeWidth?: SvgNumber | undefined;
	strokeLinecap?: "butt" | "round" | "square" | undefined;
	strokeLinejoin?: "miter" | "round" | "bevel" | undefined;
	strokeMiterlimit?: SvgNumber | undefined;
	strokeDasharray?: SvgNumber | undefined;
	strokeDashoffset?: SvgNumber | undefined;
	strokeOpacity?: SvgNumber | undefined;
	opacity?: SvgNumber | undefined;
};
/** `width` and `height` are the intrinsic size in CSS pixels; `style` sizes the box like an `img`. */
export type SvgProps = CommonProps &
	SvgPresentation & {
		width?: SvgNumber | undefined;
		height?: SvgNumber | undefined;
		viewBox?: string | undefined;
		preserveAspectRatio?: string | undefined;
		"aria-label"?: string | undefined;
		xmlns?: string | undefined;
		children?: PdfNode | undefined;
	};
/** A shape inside `<svg>`; `Geometry` names its numeric attributes. */
export type SvgShapeProps<Geometry extends string = never> = SvgPresentation & {
	[Attribute in Geometry]?: SvgNumber | undefined;
} & {
	key?: string | number | undefined;
	id?: string | undefined;
	transform?: string | undefined;
	style?: SvgPresentation | string | undefined;
	children?: never;
};
/** `level` defaults to M and `quietZone` to 4 modules; children draw over the symbol in a 100 by 100 box. */
export type QRCodeProps = CommonProps & {
	value: string;
	level?: "L" | "M" | "Q" | "H" | undefined;
	quietZone?: number | undefined;
	alt?: string | undefined;
	children?: PdfNode | undefined;
};
/** `moduleWidth` (default 1) and `height` (default 36) are points; `text` (default true) prints the value. */
export type BarcodeProps = CommonProps & {
	value: string;
	moduleWidth?: number | undefined;
	height?: number | undefined;
	text?: boolean | undefined;
	quietZone?: number | undefined;
	alt?: string | undefined;
};
/** With `RenderOptions.outline`, `data-bookmark` replaces the heading's bookmark title, or `false` skips it. */
export type HeadingProps = BlockProps & { "data-bookmark"?: string | false | undefined };
export type PdfProps =
	| BlockProps
	| VoidProps
	| ImageProps
	| LinkProps
	| HeadingProps
	| SvgProps
	| SvgShapeProps
	| OrderedListProps
	| ListItemProps;
