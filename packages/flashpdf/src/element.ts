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
/** `src` is the PNG or JPEG file bytes; `alt` is stored for accessibility tagging. */
export type ImageProps = VoidProps & { src: Uint8Array; alt?: string | undefined };
/** `href` must be an absolute `http:`, `https:`, or `mailto:` URL. */
export type LinkProps = BlockProps & { href: string };
export type PdfProps =
	| BlockProps
	| VoidProps
	| ImageProps
	| LinkProps
	| OrderedListProps
	| ListItemProps;
