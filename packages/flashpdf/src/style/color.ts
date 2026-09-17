export function color(value: unknown): [number, number, number] {
	if (typeof value !== "string") throw new Error("invalid color");
	const hex = value.trim().toLowerCase();
	const named: Record<string, string> = {
		black: "#000000",
		white: "#ffffff",
		red: "#ff0000",
		green: "#008000",
		blue: "#0000ff",
	};
	const source = named[hex] ?? hex;
	const match = /^#([\da-f]{3}|[\da-f]{6})$/.exec(source);
	if (!match)
		throw new Error(
			hex === "transparent"
				? "transparent applies only to background and border color"
				: `unsupported color: ${hex}`,
		);
	const group = match[1]!;
	const value6 = group.length === 3 ? [...group].map((part) => part + part).join("") : group;
	return [
		Number.parseInt(value6.slice(0, 2), 16),
		Number.parseInt(value6.slice(2, 4), 16),
		Number.parseInt(value6.slice(4, 6), 16),
	];
}
