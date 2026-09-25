import { PageNumber, render, TotalPages, type EmbeddedFont } from "@pettersen3008/flashpdf";

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
void render(invoice, {
	footer: (
		<footer>
			Page <PageNumber /> of <TotalPages />
		</footer>
	),
});
// @ts-expect-error A PDF document starts with one or more React elements.
void render("bare text");
// @ts-expect-error FlashPDF only accepts its documented intrinsic elements.
const _unsupported = <button>Print</button>;
// @ts-expect-error Unknown intrinsic properties cannot reach runtime validation.
const _unknownProperty = <main potato={1}>Invoice</main>;
// @ts-expect-error Grid is not part of FlashPDF's supported CSS subset.
const _grid = <main style={{ display: "grid" }}>Invoice</main>;
// @ts-expect-error Margin and padding take one to four lengths, never "auto".
const _auto = <main style={{ margin: "auto" }}>Invoice</main>;
const _rich = (
	<p>
		Hello <b>World</b>
		<br />
		<strong style={{ color: "#c00" }}>again</strong>
	</p>
);
// @ts-expect-error Line breaks have no children.
const _brChildren = <br>x</br>;
void import("@pettersen3008/flashpdf/jsx-runtime");
