import { render, type RenderOptions } from "@pettersen3008/flashpdf";

export type Invoice = {
	number: string;
	issued: string;
	due: string;
	seller: { name: string; address: string };
	customer: { name: string; address: string };
	currency: string;
	items: readonly { description: string; quantity: number; unitPrice: number }[];
	taxRate: number;
};

const ink = "#1a1a1a";
const muted = "#667085";
const rule = "#d0d5dd";

function money(currency: string, amount: number) {
	return `${currency} ${amount.toFixed(2)}`;
}

function rounded(amount: number) {
	return Math.round((amount + Number.EPSILON) * 100) / 100;
}

/** A row of the line-item table. Only a flex row may size its columns. */
function Line({
	cells,
	bold,
	color,
}: {
	cells: readonly string[];
	bold?: boolean;
	color?: string;
}) {
	return (
		<div
			style={{
				display: "flex",
				flexDirection: "row",
				fontWeight: bold ? "bold" : undefined,
				color,
			}}
		>
			<span style={{ flex: 1 }}>{cells[0]}</span>
			<span style={{ width: "60pt", textAlign: "right" }}>{cells[1]}</span>
			<span style={{ width: "90pt", textAlign: "right" }}>{cells[2]}</span>
			<span style={{ width: "90pt", textAlign: "right" }}>{cells[3]}</span>
		</div>
	);
}

function Party({ title, name, address }: { title: string; name: string; address: string }) {
	return (
		<div style={{ flex: 1 }}>
			<p style={{ color: muted, fontSize: 9 }}>{title}</p>
			<p style={{ fontWeight: "bold" }}>{name}</p>
			<p>{address}</p>
		</div>
	);
}

export function nativeInvoice(invoice: Invoice, options?: Pick<RenderOptions, "fonts">) {
	const amounts = invoice.items.map((item) => rounded(item.quantity * item.unitPrice));
	const net = amounts.reduce((total, amount) => total + amount, 0);
	const tax = rounded(net * invoice.taxRate);

	return render(
		<main
			style={{
				display: "flex",
				flexDirection: "column",
				gap: 16,
				fontSize: 10,
				color: ink,
				fontFamily: options?.fonts?.[0]?.family,
			}}
		>
			<div style={{ display: "flex", flexDirection: "row" }}>
				<div style={{ flex: 1 }}>
					<h1>Invoice</h1>
				</div>
				<div style={{ width: "170pt" }}>
					<p style={{ textAlign: "right" }}>
						No. <b>{invoice.number}</b>
					</p>
					<p style={{ textAlign: "right", color: muted }}>Issued {invoice.issued}</p>
					<p style={{ textAlign: "right", color: muted }}>Due {invoice.due}</p>
				</div>
			</div>

			<div style={{ display: "flex", flexDirection: "row" }}>
				<Party title="FROM" name={invoice.seller.name} address={invoice.seller.address} />
				<Party title="BILL TO" name={invoice.customer.name} address={invoice.customer.address} />
			</div>

			{/* Line items are plain block children, so a long invoice paginates
          without holding the whole table in memory. */}
			<div>
				<Line cells={["Description", "Qty", "Unit", "Amount"]} bold color={muted} />
				<div style={{ borderWidth: 1, borderColor: rule, marginTop: 2, marginBottom: 4 }} />
				{invoice.items.map((item, index) => (
					<Line
						key={item.description}
						cells={[
							item.description,
							String(item.quantity),
							money(invoice.currency, item.unitPrice),
							money(invoice.currency, amounts[index]!),
						]}
					/>
				))}
			</div>

			{/* A decorated box is atomic, so the totals block never splits. */}
			<div
				style={{
					breakInside: "avoid",
					padding: 10,
					background: "#f9fafb",
					borderWidth: 1,
					borderColor: rule,
				}}
			>
				<Line cells={["", "", "Net", money(invoice.currency, net)]} color={muted} />
				<Line
					cells={[
						"",
						"",
						`Tax ${(invoice.taxRate * 100).toFixed(0)}%`,
						money(invoice.currency, tax),
					]}
					color={muted}
				/>
				<Line cells={["", "", "Total due", money(invoice.currency, rounded(net + tax))]} bold />
			</div>
		</main>,
		{ pageFormat: "A4", margin: 36, fonts: options?.fonts },
	);
}
