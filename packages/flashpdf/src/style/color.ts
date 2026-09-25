const NAMED = new Map<string, string>();
for (const [, name, hex] of `
aliceblue f0f8ff antiquewhite faebd7 aqua 00ffff aquamarine 7fffd4 azure f0ffff beige f5f5dc
bisque ffe4c4 black 000000 blanchedalmond ffebcd blue 0000ff blueviolet 8a2be2 brown a52a2a
burlywood deb887 cadetblue 5f9ea0 chartreuse 7fff00 chocolate d2691e coral ff7f50
cornflowerblue 6495ed cornsilk fff8dc crimson dc143c cyan 00ffff darkblue 00008b darkcyan 008b8b
darkgoldenrod b8860b darkgray a9a9a9 darkgreen 006400 darkgrey a9a9a9 darkkhaki bdb76b
darkmagenta 8b008b darkolivegreen 556b2f darkorange ff8c00 darkorchid 9932cc darkred 8b0000
darksalmon e9967a darkseagreen 8fbc8f darkslateblue 483d8b darkslategray 2f4f4f
darkslategrey 2f4f4f darkturquoise 00ced1 darkviolet 9400d3 deeppink ff1493 deepskyblue 00bfff
dimgray 696969 dimgrey 696969 dodgerblue 1e90ff firebrick b22222 floralwhite fffaf0
forestgreen 228b22 fuchsia ff00ff gainsboro dcdcdc ghostwhite f8f8ff gold ffd700 goldenrod daa520
gray 808080 green 008000 greenyellow adff2f grey 808080 honeydew f0fff0 hotpink ff69b4
indianred cd5c5c indigo 4b0082 ivory fffff0 khaki f0e68c lavender e6e6fa lavenderblush fff0f5
lawngreen 7cfc00 lemonchiffon fffacd lightblue add8e6 lightcoral f08080 lightcyan e0ffff
lightgoldenrodyellow fafad2 lightgray d3d3d3 lightgreen 90ee90 lightgrey d3d3d3 lightpink ffb6c1
lightsalmon ffa07a lightseagreen 20b2aa lightskyblue 87cefa lightslategray 778899
lightslategrey 778899 lightsteelblue b0c4de lightyellow ffffe0 lime 00ff00 limegreen 32cd32
linen faf0e6 magenta ff00ff maroon 800000 mediumaquamarine 66cdaa mediumblue 0000cd
mediumorchid ba55d3 mediumpurple 9370db mediumseagreen 3cb371 mediumslateblue 7b68ee
mediumspringgreen 00fa9a mediumturquoise 48d1cc mediumvioletred c71585 midnightblue 191970
mintcream f5fffa mistyrose ffe4e1 moccasin ffe4b5 navajowhite ffdead navy 000080 oldlace fdf5e6
olive 808000 olivedrab 6b8e23 orange ffa500 orangered ff4500 orchid da70d6 palegoldenrod eee8aa
palegreen 98fb98 paleturquoise afeeee palevioletred db7093 papayawhip ffefd5 peachpuff ffdab9
peru cd853f pink ffc0cb plum dda0dd powderblue b0e0e6 purple 800080 rebeccapurple 663399
red ff0000 rosybrown bc8f8f royalblue 4169e1 saddlebrown 8b4513 salmon fa8072 sandybrown f4a460
seagreen 2e8b57 seashell fff5ee sienna a0522d silver c0c0c0 skyblue 87ceeb slateblue 6a5acd
slategray 708090 slategrey 708090 snow fffafa springgreen 00ff7f steelblue 4682b4 tan d2b48c
teal 008080 thistle d8bfd8 tomato ff6347 turquoise 40e0d0 violet ee82ee wheat f5deb3
white ffffff whitesmoke f5f5f5 yellow ffff00 yellowgreen 9acd32
`.matchAll(/([a-z]+)\s+([\da-f]{6})/g))
	NAMED.set(name!, hex!);

type Rgb = [number, number, number];

function opaque(alpha: number) {
	if (Math.abs(alpha - 1) > 1e-6) throw new Error("opacity is not supported");
}
/** `50%` → 0.5, otherwise the number scaled by `scale`; NaN for anything else. */
function fraction(token: string, scale: number) {
	const percent = token.endsWith("%");
	const amount = Number(percent ? token.slice(0, -1) : token);
	return percent ? amount / 100 : amount / scale;
}
function byte(value: number) {
	return Math.round(Math.min(1, Math.max(0, value)) * 255);
}

function hex(source: string): Rgb | undefined {
	const match = /^#([\da-f]{3,4}|[\da-f]{6}|[\da-f]{8})$/.exec(source);
	if (!match) return undefined;
	let digits = match[1]!;
	if (digits.length <= 4) digits = [...digits].map((digit) => digit + digit).join("");
	if (digits.length === 8) opaque(Number.parseInt(digits.slice(6), 16) / 255);
	return [0, 2, 4].map((index) => Number.parseInt(digits.slice(index, index + 2), 16)) as Rgb;
}

function hsl(hue: number, saturation: number, lightness: number): Rgb {
	const h = ((hue % 360) + 360) % 360;
	const a = saturation * Math.min(lightness, 1 - lightness);
	const channel = (n: number) => {
		const k = (n + h / 30) % 12;
		return byte(lightness - a * Math.max(-1, Math.min(k - 3, 9 - k, 1)));
	};
	return [channel(0), channel(8), channel(4)];
}

function functional(source: string): Rgb | undefined {
	const match = /^(rgba?|hsla?)\((.*)\)$/.exec(source);
	if (!match) return undefined;
	const [first, second, third, alpha, extra] = match[2]!.split(/\s*[,/]\s*|\s+/).filter(Boolean);
	if (third === undefined || extra !== undefined) return undefined;
	if (alpha !== undefined) opaque(fraction(alpha, 1));
	const rgb: Rgb = match[1]!.startsWith("rgb")
		? [byte(fraction(first!, 255)), byte(fraction(second!, 255)), byte(fraction(third, 255))]
		: hsl(Number(first!.replace(/deg$/, "")), fraction(second!, 100), fraction(third, 100));
	return rgb.some(Number.isNaN) ? undefined : rgb;
}

export function color(value: unknown): Rgb {
	if (typeof value !== "string") throw new Error(`invalid color: ${String(value)}`);
	const source = value.trim().toLowerCase();
	const named = NAMED.get(source);
	const rgb = hex(named ? `#${named}` : source) ?? functional(source);
	if (!rgb)
		throw new Error(
			source === "transparent"
				? "transparent applies only to background and border color"
				: `unsupported color: ${source}`,
		);
	return rgb;
}
