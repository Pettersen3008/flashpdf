export type Length = number | "0" | `${number}${"pt" | "px" | "em" | "rem"}`;
export type Width = number | `${number}${"pt" | "px" | "%"}`;

export type Style = {
	display?: "block" | "flex";
	flexDirection?: "row" | "column";
	flex?: number;
	width?: Width;
	margin?: number | string;
	marginTop?: Length;
	marginRight?: Length;
	marginBottom?: Length;
	marginLeft?: Length;
	padding?: number | string;
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
export type TextChildren =
	| string
	| number
	| bigint
	| boolean
	| null
	| undefined
	| readonly TextChildren[];
export type CommonProps = {
	key?: string | number | undefined;
	id?: string | undefined;
	className?: string | undefined;
	style?: StyleInput | undefined;
};
export type BlockProps = CommonProps & { children?: PdfNode | undefined };
export type TextProps = CommonProps & { children?: TextChildren | undefined };
export type HrProps = CommonProps & { children?: never };
export type PdfProps = BlockProps | TextProps | HrProps;
