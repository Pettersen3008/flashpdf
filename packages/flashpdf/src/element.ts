export type Length = number | "0" | `${number}${"pt" | "px" | "em" | "rem"}`;
export type Width = number | `${number}${"pt" | "px" | "%"}`;
export type Edges =
	| Length
	| `${Length} ${Length}`
	| `${Length} ${Length} ${Length}`
	| `${Length} ${Length} ${Length} ${Length}`;
/** `auto` horizontal margins centre a block with a width; flex items absorb free space with them. */
export type Margin = Length | "auto";
export type MarginEdges =
	| Margin
	| `${Margin} ${Margin}`
	| `${Margin} ${Margin} ${Margin}`
	| `${Margin} ${Margin} ${Margin} ${Margin}`;
export type Alignment =
	| "normal"
	| "stretch"
	| "flex-start"
	| "start"
	| "flex-end"
	| "end"
	| "center";

export type Style = {
	display?: "block" | "flex";
	flexDirection?: "row" | "column";
	flexWrap?: "nowrap";
	justifyContent?:
		| "normal"
		| "flex-start"
		| "start"
		| "flex-end"
		| "end"
		| "center"
		| "space-between"
		| "space-around"
		| "space-evenly";
	alignItems?: Alignment;
	alignSelf?: Alignment | "auto";
	/** `none`, `auto`, a grow factor, or grow and shrink factors with an optional basis. */
	flex?: number | "auto" | "none" | `${number} ${string}`;
	flexGrow?: number;
	flexShrink?: number;
	flexBasis?: Width | "auto";
	width?: Width | "auto";
	minWidth?: Width | "auto";
	maxWidth?: Width | "none";
	/** A percent needs a parent with a fixed `height`. */
	height?: Width | "auto";
	minHeight?: Width | "auto";
	maxHeight?: Width | "none";
	boxSizing?: "content-box" | "border-box";
	margin?: MarginEdges;
	marginTop?: Margin;
	marginRight?: Margin;
	marginBottom?: Margin;
	marginLeft?: Margin;
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
	textAlign?: "left" | "center" | "right";
	textDecoration?: "none" | "underline" | "line-through" | "underline line-through";
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
/** `src` is the PNG or JPEG file bytes; `alt` is stored for accessibility tagging. */
export type ImageProps = VoidProps & { src: Uint8Array; alt?: string | undefined };
/** `href` must be an absolute `http:`, `https:`, or `mailto:` URL. */
export type LinkProps = BlockProps & { href: string };
export type PdfProps = BlockProps | VoidProps | ImageProps | LinkProps;
