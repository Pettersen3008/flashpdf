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
					<p className={styles.muted}>Invoice #{invoice.number}</p>
				</div>
				<p style={{ textAlign: "right", fontSize: 12 }}>Due {invoice.dueDate}</p>
			</header>
			<section className={styles.items}>
				{invoice.items.map((item) => (
					<div className={styles.item} key={item.id}>
						<span>{item.name}</span>
						<span className={styles.amount}>{item.total}</span>
					</div>
				))}
			</section>
		</main>,
		{ pageFormat: "A4", margin: 36, stylesheets: [invoiceCss] },
	);
}
