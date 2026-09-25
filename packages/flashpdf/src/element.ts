export type Length = number | "0" | `${number}${"pt" | "px" | "em" | "rem"}`;
export type Width = number | `${number}${"pt" | "px" | "%"}`;
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
