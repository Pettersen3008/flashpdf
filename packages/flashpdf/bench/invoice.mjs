import { readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

// Same 80-line invoice as the Takumi comparison, with fonts supplied offline.
export async function invoice(root, fonts, tagged) {
	const { jsx, jsxs } = await import(pathToFileURL(join(root, "dist/jsx-runtime.js")));
	const { PageNumber, TotalPages } = await import(pathToFileURL(join(root, "dist/page.js")));
	const items = Array.from({ length: 80 }, (_, i) => ({
		description: `Line item ${i + 1}: professional services rendered during the billing period`,
		qty: (i % 4) + 1,
		unit: 120 + i * 7,
	}));
	const right = { textAlign: "right" };
	const options = {
		fonts: [
			{
				family: "Inter",
				regular: readFileSync(join(fonts, "inter-400.ttf")),
				bold: readFileSync(join(fonts, "inter-700.ttf")),
			},
		],
		...(tagged && { tagged: true, metadata: { language: "en-US" } }),
		footer: jsxs("footer", {
			style: { fontFamily: "Inter", fontSize: "10px", color: "#6b7280", textAlign: "center" },
			children: ["Page ", jsx(PageNumber, {}), " of ", jsx(TotalPages, {})],
		}),
	};
	return {
		options,
		create: () =>
			jsxs("main", {
				style: {
					display: "flex",
					flexDirection: "column",
					fontFamily: "Inter",
					fontSize: "13px",
					color: "#111827",
				},
				children: [
					jsxs("div", {
						style: {
							display: "flex",
							justifyContent: "space-between",
							borderBottom: "1px solid #d1d5db",
							paddingBottom: "16px",
							marginBottom: "16px",
						},
						children: [
							jsx("h1", {
								style: { margin: 0, fontSize: "24px", lineHeight: "32px", fontWeight: "bold" },
								children: "Invoice INV-2026-001",
							}),
							jsx("p", { style: { margin: 0 }, children: "Due August 31, 2026" }),
						],
					}),
					jsx("div", {
						style: { display: "flex", flexDirection: "column" },
						children: items.map((item) =>
							jsxs("div", {
								style: {
									display: "flex",
									paddingTop: "4px",
									paddingBottom: "4px",
									breakInside: "avoid",
								},
								children: [
									jsx("span", {
										style: { flex: 1, paddingRight: "16px" },
										children: item.description,
									}),
									jsx("span", { style: { width: "32px", ...right }, children: item.qty }),
									jsx("span", {
										style: { width: "90px", ...right },
										children: `$${(item.qty * item.unit).toFixed(2)}`,
									}),
								],
							}),
						),
					}),
					jsxs("div", {
						style: {
							display: "flex",
							justifyContent: "space-between",
							borderTop: "1px solid #d1d5db",
							marginTop: "16px",
							paddingTop: "8px",
							fontWeight: "bold",
						},
						children: [
							jsx("span", { children: "Total" }),
							jsx("span", {
								children: `$${items.reduce((sum, item) => sum + item.qty * item.unit, 0).toFixed(2)}`,
							}),
						],
					}),
				],
			}),
	};
}
