import { render } from "@pettersen3008/flashpdf";

import styles from "./invoice.module.css";
import invoiceCss from "./invoice.module.css?inline";

type Invoice = {
	title: string;
	number: string;
	dueDate: string;
	items: readonly { id: string; name: string; total: string }[];
};

export function cssInvoice(invoice: Invoice) {
	return render(
		<main className={styles.invoice}>
			<header className={styles.header}>
				<div>
					<h1>{invoice.title}</h1>
					<p className={styles.muted}>
						Invoice <b>#{invoice.number}</b>
					</p>
				</div>
				<p style={{ textAlign: "right", fontSize: 12 }}>
					Due <b>{invoice.dueDate}</b>
				</p>
			</header>
			<table className={styles.items}>
				<thead>
					<tr>
						<th>Item</th>
						<th className={styles.amount}>Amount</th>
					</tr>
				</thead>
				<tbody>
					{invoice.items.map((item) => (
						<tr key={item.id}>
							<td>{item.name}</td>
							<td className={styles.amount}>{item.total}</td>
						</tr>
					))}
				</tbody>
			</table>
		</main>,
		{ pageFormat: "A4", margin: 36, stylesheets: [invoiceCss] },
	);
}
