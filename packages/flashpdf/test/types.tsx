import { render, type EmbeddedFont } from "@pettersen3008/flashpdf";

const invoice = (
	<main className="invoice" style={{ padding: "18pt", backgroundColor: "#fff" }}>
		<header style={{ display: "flex", flexDirection: "row" }}>
			<h1>Invoice</h1>
			<p style={{ textAlign: "right" }}>Due today</p>
		</header>
		<section>
			{["Consulting", "Support"].map((item) => (
				<div key={item} style={{ display: "flex", flexDirection: "row" }}>
					<span style={{ flex: 1 }}>{item}</span>
					<span>$100</span>
				</div>
			))}
		</section>
	</main>
);
void render(invoice, { pageFormat: "A4", margin: 36, stylesheets: [".invoice { color: #222 }"] });
void render([invoice]);
const font: EmbeddedFont = { family: "Invoice", regular: new Uint8Array(), bold: new Uint8Array() };
void render(invoice, { fonts: [font] });
// @ts-expect-error A PDF document starts with one or more React elements.
void render("bare text");
// @ts-expect-error React owns the JSX runtime; FlashPDF only consumes its nodes.
void import("@pettersen3008/flashpdf/jsx-runtime");
