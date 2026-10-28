import {
	Barcode,
	PageNumber,
	QRCode,
	render,
	TotalPages,
	type ElementData,
	type EmbeddedFont,
	type Style,
} from "@pettersen3008/flashpdf";

const _data: ElementData = {
	type: "main",
	props: { className: "invoice", children: [{ type: "p", props: { children: "Total" } }] },
};
void render(_data);
void render("<main><img src=logo></main>", {
	images: { logo: new Uint8Array(1) },
	footer: "<p>Acme</p>",
});
// @ts-expect-error An images entry holds bytes, not a URL.
void render("<main></main>", { images: { logo: "logo.png" } });
// @ts-expect-error An element's type is a supported tag.
const _dataTag: ElementData = { type: "marquee", props: {} };

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
// @ts-expect-error A PDF document is elements or an HTML string, not a number.
void render(42);
// @ts-expect-error FlashPDF only accepts its documented intrinsic elements.
const _unsupported = <button>Print</button>;
// @ts-expect-error Unknown intrinsic properties cannot reach runtime validation.
const _unknownProperty = <main potato={1}>Invoice</main>;
const _gridStyle: Style = {
	display: "grid",
	gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
	gridTemplateRows: "20% 1fr",
	gridAutoColumns: 30,
	gridAutoRows: "minmax(10pt, auto)",
	gridAutoFlow: "column",
	gap: "2pt 4pt",
	justifyItems: "start",
	alignContent: "space-between",
};
const _grid = (
	<main style={_gridStyle}>
		<div
			style={{
				gridColumn: "2 / span 2",
				gridRowStart: -2,
				gridRowEnd: -1,
				justifySelf: "end",
				alignSelf: "center",
			}}
		>
			Invoice
		</div>
	</main>
);
// @ts-expect-error Dense placement is outside the bounded grid subset.
const _denseGrid: Style = { gridAutoFlow: "row dense" };
// @ts-expect-error Grid areas are outside the bounded grid subset.
const _gridArea: Style = { gridArea: "invoice" };
const _decorated = (
	<main style={{ borderRadius: "4pt 8pt / 50%", borderTopStyle: "dashed", opacity: "50%" }}>x</main>
);
const _shadow = (
	<main
		style={{
			boxShadow: "0 1px 0 2px #000",
			backgroundImage: "linear-gradient(red, blue)",
			backgroundSize: "auto",
			backgroundPosition: "0% 0%",
		}}
	>
		Invoice
	</main>
);
// @ts-expect-error Only none, solid, dashed, and dotted borders are drawn.
const _double = <main style={{ borderTopStyle: "double" }}>Invoice</main>;
// @ts-expect-error Padding takes one to four lengths, never "auto".
const _auto = <main style={{ padding: "auto" }}>Invoice</main>;
const _flex = (
	<header style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
		<div style={{ width: 60, height: 40, margin: "0 auto", boxSizing: "border-box" }} />
		<p style={{ flex: "1 1 0%", maxWidth: "50%", alignSelf: "flex-end" }}>Due</p>
	</header>
);
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
const _vector = (
	<svg viewBox="0 0 24 24" width={32} aria-label="Paid" style={{ color: "#0a7", marginBottom: 4 }}>
		<title>Paid</title>
		<g transform="rotate(45 12 12)" opacity={0.8}>
			<rect width={24} height={24} rx={4} fill="none" stroke="currentColor" strokeWidth={2} />
			<path d="M7 12l3 3 7-7" fill="none" stroke="currentColor" strokeLinecap="round" />
		</g>
	</svg>
);
// @ts-expect-error Shapes take no children.
const _shapeChildren = <circle r={2}>x</circle>;
// @ts-expect-error A path needs its d.
const _bareShape = <path />;
const _codes = (
	<main>
		<QRCode value="https://example.com/pay" level="Q" alt="Pay" style={{ width: 90 }}>
			<rect x={40} y={40} width={20} height={20} fill="#fff" />
		</QRCode>
		<Barcode value="INV-42" moduleWidth={1.2} height={30} text={false} />
	</main>
);
// @ts-expect-error A QR code needs its value.
const _qrWithoutValue = <QRCode level="M" />;
// @ts-expect-error The level is L, M, Q, or H.
const _qrLevel = <QRCode value="x" level="X" />;
const _outline = (
	<main>
		<h2 id="terms" data-bookmark="Terms of payment">
			Terms
		</h2>
		<h3 data-bookmark={false}>Skipped</h3>
		<a href="#terms">terms</a>
	</main>
);
void render(_outline, { outline: true });
// @ts-expect-error A bookmark title is a string, or false to skip the heading.
const _bookmarkTrue = <h2 data-bookmark={true}>Terms</h2>;
// @ts-expect-error The outline option is a boolean.
void render(_outline, { outline: "yes" });
void import("@pettersen3008/flashpdf/jsx-runtime");
