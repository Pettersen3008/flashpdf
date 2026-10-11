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
void render(invoice, { pageFormat: "A5", orientation: "landscape", margin: "20mm 15mm" });
void render(invoice, {
	pageFormat: { width: "210mm", height: 400 },
	margin: { top: "45mm", right: "2cm", bottom: "1in", left: 72 },
});
// @ts-expect-error Page lengths take a unit; a bare numeric string is not points.
void render(invoice, { margin: { top: "45", right: 0, bottom: 0, left: 0 } });
const font: EmbeddedFont = { family: "Invoice", regular: new Uint8Array(), bold: new Uint8Array() };
void render(invoice, { fonts: [font] });
void render(invoice, {
	footer: (
		<footer>
			Page <PageNumber /> of <TotalPages />
		</footer>
	),
	firstPageHeader: [<h1 key="name">Acme</h1>, <p key="address">Storgata 1</p>],
	firstPageFooter: null,
});
// @ts-expect-error Only the first-page variants take null.
void render(invoice, { header: null });
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
const _table = (
	<table style={{ width: "80%" }}>
		<thead>
			<tr>
				<th>Item</th>
				<th style={{ width: "60pt", textAlign: "right" }}>Total</th>
			</tr>
		</thead>
		<tbody>
			<tr style={{ background: "#f4f4f5" }}>
				<td style={{ padding: 2 }}>Consulting</td>
				<td style={{ textAlign: "right" }}>$100</td>
			</tr>
		</tbody>
	</table>
);
const _spans = (
	<table style={{ borderCollapse: "collapse" }}>
		<tr>
			<td colSpan={2} rowSpan={1} style={{ verticalAlign: "middle" }}>
				x
			</td>
		</tr>
	</table>
);
// @ts-expect-error Only table cells span columns.
const _divSpan = <div colSpan={2}>x</div>;
const _assets = (
	<main>
		<a href="https://example.com/">
			<img src={new Uint8Array()} alt="Logo" style={{ width: 80, height: "20pt" }} />
		</a>
		<p>
			Pay at{" "}
			<a href="mailto:billing@example.com" style={{ textDecoration: "none" }}>
				billing
			</a>
		</p>
	</main>
);
// @ts-expect-error Image bytes come from the host, never a path or URL.
const _imgPath = <img src="/logo.png" />;
// @ts-expect-error A link needs its href.
const _bareLink = <a>site</a>;
// @ts-expect-error Images have no children.
const _imgChildren = <img src={new Uint8Array()}>x</img>;
void import("@pettersen3008/flashpdf/jsx-runtime");
