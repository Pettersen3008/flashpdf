type Absolute = "pt" | "px" | "pc" | "in" | "cm" | "mm" | "Q";
export type Length = number | "0" | `${number}${Absolute | "em" | "rem"}`;
export type Width = number | `${number}${Absolute | "%"}`;
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
	textAlign?: "left" | "center" | "right";
	textDecoration?: "none" | "underline" | "line-through" | "underline line-through";
	color?: string;
	breakBefore?: "auto" | "page" | "avoid" | "avoid-page";
	breakAfter?: "auto" | "page" | "avoid" | "avoid-page";
	breakInside?: "auto" | "avoid" | "avoid-page";
	/** Lines of a paragraph a page break leaves before it; 2 by default. */
	orphans?: number;
	/** Lines of a paragraph a page break carries after it; 2 by default. */
	widows?: number;
	/** Table cells and rows only; `baseline` rejects. */
	verticalAlign?: "top" | "middle" | "bottom";
	borderCollapse?: "collapse" | "separate";
	/** Only 0 is supported: cells tile their row. */
	borderSpacing?: 0 | "0" | Length | `${Length} ${Length}`;
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
/** `colSpan` is 1 to 256 and `rowSpan` 1 to 65535, within the cell's row group. */
export type CellProps = BlockProps & {
	colSpan?: number | undefined;
	rowSpan?: number | undefined;
};
/** `href` must be an absolute `http:`, `https:`, or `mailto:` URL. */
export type LinkProps = BlockProps & { href: string };
export type PdfProps = BlockProps | VoidProps | ImageProps | LinkProps | CellProps;
