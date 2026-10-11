# FlashPDF guide

Setup for bundlers and browsers, fixes for common rejections, four recipes, and a map from react-pdf. The [README](../README.md) covers the API and [CSS.md](../CSS.md) covers the CSS subset.

## Bundlers and runtimes

The browser entry loads the WASM with `new URL('../wasm/flashpdf_wasm_bg.wasm', import.meta.url)`. A bundler that understands that pattern emits the file as an asset. Serve it as `application/wasm`: browsers compile it while it streams only with that type, and otherwise FlashPDF falls back to a slower path and logs a warning.

**Vite.** No plugin and no `optimizeDeps` setting. The dev server and `vite build` both work, and `tests/e2e.mjs` builds with Vite on every run.

**webpack 5.** No loader and no `experiments.asyncWebAssembly`. webpack emits the WASM as a hashed asset next to the bundle. Set `output.publicPath` to where you serve that asset, as for any webpack asset.

**Next.js 16.** A client component needs no configuration. Server code, such as a route handler on the Node.js runtime, also works with the default Turbopack build. With `next build --webpack`, the server bundle rewrites the WASM path and `render` fails with `ERR_INVALID_URL`. Keep the package out of that bundle:

```js
// next.config.mjs
export default { serverExternalPackages: ['@pettersen3008/flashpdf'] };
```

The option is harmless under Turbopack, so you can set it everywhere. The `edge` runtime is untested.

**Content Security Policy.** A browser blocks WebAssembly compilation unless `script-src` allows it. Without the directive, `render` rejects with `WebAssembly.instantiateStreaming(): Compiling or instantiating WebAssembly module violates the following Content Security policy directive because 'unsafe-eval' is not an allowed source of script`. Add `'wasm-unsafe-eval'`, which allows WebAssembly and nothing else:

```text
Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'
```

FlashPDF also loads the WASM with `fetch`, so `connect-src` must allow the origin that serves it. `'self'` covers a file that ships with your app.

Node, Bun, AWS Lambda, and Cloudflare Workers need no bundler setup. See Compatibility in the README.

## Troubleshooting

FlashPDF rejects what it cannot draw and names the element. Each row below shows the start of a real message.

| Message | Cause | Fix |
| --- | --- | --- |
| `unsupported element <em>` or `<i>` | Only regular and bold faces exist, so there is no italic. | Use `<b>` or a `<span>` with a colour. |
| `unsupported style property: fontStyle`, `unsupported CSS property: font-style` | Same. | Delete the declaration. |
| `unsupported CSS property: letter-spacing (text shaping is not implemented)` | Text has no shaping, so no tracking. | Delete the declaration. A rule that matches no rendered element is ignored. |
| `PageOverflow: the box is taller than a page`, `the table is taller than a page`, or `the image is taller than the page` | An atomic block has to fit one page. | Remove `break-inside: avoid` from the box or table, or reduce the image height. |
| `Helvetica has no glyph for "−" (U+2212)` | The default font covers WinAnsi only. `Intl.NumberFormat` writes U+2212 for negatives in `nb-NO` and U+202F as the group separator in `fr-FR`. | Embed a TTF that has the glyph, or replace it: `text.replace(/−/g, '-').replace(/ /g, ' ')`. |
| `src must be the image's PNG or JPEG bytes as a Uint8Array` | `img` does not take a URL, path, or `data:` URL. | Read or fetch the file and pass its bytes. |
| `unsupported element <ul>` | There are no lists. | One `<p>` per item that starts with `•`. |
| `unsupported style property: borderRadius`, `position`, `opacity`, `justifyContent`, `alignItems` | No equivalent exists. | Remove it. [CSS.md](../CSS.md) lists every limit. |
| `height applies only to <img>` | Block height comes from content. | Remove `height`. Space blocks with `margin` or `padding`. |
| `rowSpan 2 runs past the last row of its <tbody>` | Rows that a `rowSpan` joins must sit in one row group. | Move the rows into one `tbody`, or lower `rowSpan`. |
| `page tokens are only valid in a header or footer` | `PageNumber` and `TotalPages` only resolve there. | Move them into a `header`, `footer`, `firstPageHeader`, or `firstPageFooter` option. |
| `PageOverflow: with footer, the header and footer take more than half the page` | The body keeps at least half the page between the margins. | Shorten the header or footer, or move the tall part into the body. |
| `href must be an absolute http, https, or mailto URL` | Links cannot be relative or `#anchors`. | Use a full URL. |
| `breakBefore: left is not supported (pages have no sides; use page)` | `left`, `right`, `recto`, and `verso` pick a page side, and pages have none. | Use `page`. |
| `invalid orphans: 0 (use a whole number of lines from 1)` | `orphans` and `widows` count lines. | Use 1 to break wherever the page ends. |
| `nested break-before is not implemented` | Flex rows, table rows, and `break-inside: avoid` groups never break inside. | Put the break on the flex row, the table, or the group. |

## Recipes

### Totals table

Put the totals in the items table, with each label spanning the columns left of the amounts, so the numbers line up with the amount column. Keep the grand total out of `tfoot`, because `tfoot` rows repeat on every page.

```tsx
const money = new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' });
const format = (cents: number) => money.format(cents / 100);
const right = { textAlign: 'right' } as const;

const subtotal = items.reduce((sum, item) => sum + item.cents, 0);
const tax = Math.round(subtotal * 0.25);

<table>
  <thead>
    <tr>
      <th>Description</th>
      <th style={{ width: '40pt', textAlign: 'right' }}>Qty</th>
      <th style={{ width: '90pt', textAlign: 'right' }}>Amount</th>
    </tr>
  </thead>
  <tbody>
    {items.map((item) => (
      <tr key={item.id}>
        <td>{item.description}</td>
        <td style={right}>{item.quantity}</td>
        <td style={right}>{format(item.cents)}</td>
      </tr>
    ))}
  </tbody>
  <tbody style={{ breakInside: 'avoid' }}>
    <tr>
      <td colSpan={2} style={right}>Subtotal</td>
      <td style={right}>{format(subtotal)}</td>
    </tr>
    <tr>
      <td colSpan={2} style={right}>VAT 25%</td>
      <td style={right}>{format(tax)}</td>
    </tr>
    <tr>
      <th colSpan={2} style={right}>Total</th>
      <th style={right}>{format(subtotal + tax)}</th>
    </tr>
  </tbody>
</table>
```

Add amounts as integer cents and round once. `breakInside: 'avoid'` keeps the three rows on one page, and `breakBefore: 'avoid'` keeps them with the last item rows, so the totals never start a page alone.

### Page breaks

Pages break as in CSS: between blocks, between table rows, and between the lines of a paragraph, keeping at least two lines on each side (`orphans` and `widows`). A box with a border or background that crosses the page end splits there, drawing its top edge on the first page and its bottom edge on the last.

```tsx
<section style={{ breakBefore: 'page' }}>        {/* start on a new page */}
  <h2>Payment terms</h2>                          {/* stays with the next lines */}
  <p style={{ orphans: 3, widows: 3 }}>…</p>      {/* three lines on each side of a break */}
  <div style={{ breakInside: 'avoid', border: '1pt solid #d0d5dd', padding: 8 }}>
    Bank details, kept on one page
  </div>
</section>
```

Headings `h1` to `h6` default to `breakAfter: 'avoid'`, so a heading moves to the next page when the first two lines of its paragraph do not fit below it. That leaves space at the bottom of the page on purpose; set `breakAfter: 'auto'` on the heading to fill it instead. `breakAfter: 'avoid'` on a `tr` keeps it with the next row, under the repeated header.

When avoid rules leave no break point on a page, FlashPDF breaks anyway rather than lose content, first ignoring `orphans` and `widows`, then the avoid rules. It holds back at most one page of content for these decisions, so a run of avoided breaks taller than a page breaks where the page ends. A `breakInside: 'avoid'` box taller than a page rejects with `PageOverflow`.

### Page X of Y footer and a first-page letterhead

```tsx
import { PageNumber, TotalPages, render } from '@pettersen3008/flashpdf';

const pdf = await render(<Invoice />, {
  firstPageHeader: (
    <header style={{ backgroundColor: '#eef3fb', padding: '10pt', marginBottom: '12pt' }}>
      <img src={logo} alt="Acme Supply AS" style={{ width: 120 }} />
    </header>
  ),
  header: <header style={{ fontSize: '9pt', textAlign: 'right' }}>Invoice 2026-0042</header>,
  footer: (
    <footer style={{ display: 'flex', gap: '12pt', fontSize: '8pt', borderTop: '1px solid #ccc' }}>
      <p>Acme Supply AS<br />Storgata 1, 0150 Oslo</p>
      <p><b>Bank</b><br />IBAN NO93 8601 1117 947</p>
      <p style={{ textAlign: 'right' }}>Org. no. 912 345 678<br />Page <PageNumber /> of <TotalPages /></p>
    </footer>
  ),
});
```

A header or footer takes anything the body takes and paints on every page, as an artifact in tagged mode. `firstPageHeader` and `firstPageFooter` replace them on page one; pass `null` to leave page one without one. Each page reserves the height of the header and footer it paints, so the body starts lower under a tall letterhead.

The height is measured once, before the body, with every page number counted as ten digits. Each page then lays out its header and footer again with its real numbers, so alignment fits the actual digits. Give a page number room for ten digits on its line, or the header or footer reserves an extra line it does not paint.

### Payment QR code

FlashPDF draws images and does not make them. Create the PNG with any QR library and pass the bytes to `img`. This example uses the `qrcode` package, which FlashPDF does not depend on:

```tsx
import QRCode from 'qrcode';

const qr = new Uint8Array(await QRCode.toBuffer('https://pay.example.com/invoice/2026-0042', { margin: 1 }));

<img src={qr} alt="Scan to pay" style={{ width: 90 }} />
```

`width` is in points. FlashPDF accepts 8-bit, non-interlaced PNGs. An encoder that writes 1-bit or interlaced PNGs makes `render` reject the image naming the feature, so re-encode it as 8-bit.

## Migrating from react-pdf

| react-pdf | FlashPDF |
| --- | --- |
| `Document` | No wrapper. Pass the root element, or an array of roots, to `render`. `metadata` sets title, author, subject, keywords, and language. |
| `Page size="A4" orientation="landscape"` | `pageFormat: 'A4'` (also A3, A5, Letter, Legal, or `{ width, height }`) and `orientation: 'landscape'`, or `@page { size: A4 landscape }`. One size for the whole document. |
| `Page style={{ padding }}` | `margin`: one to four lengths, such as `'20mm 15mm'`, or `{ top, right, bottom, left }`, or `@page { margin }`. |
| A second `Page`, or `break` | `breakBefore: 'page'` on a direct child of the root. |
| `View` | `div`, `section`, `article`, `header`, `footer`, or `main`. |
| `Text` | `p`, `span`, `b`, `strong`, or `h1` to `h6`. A bare string inside a block also works. A nested `Text` becomes a `span` or `b` inside a `p`. |
| `Image src={url}` | `img src={bytes} alt="…"`. You read or fetch the PNG or JPEG bytes. |
| `Link src` | `a href`, absolute `http:`, `https:`, or `mailto:`. |
| `StyleSheet.create` | A `style` object, or CSS strings in `stylesheets`. |
| `Font.register` | `fonts: [{ family, regular, bold }]` with TTF bytes. Regular and bold only. |
| `fixed` header or footer | The `header` and `footer` options, which take any element tree. `firstPageHeader` and `firstPageFooter` cover a different first page. |
| `render={({ pageNumber, totalPages }) => …}` | `<PageNumber />` and `<TotalPages />` anywhere inside a header or footer option. |
| `wrap={false}` | `breakInside: 'avoid'`. |
| `orphans`, `widows`, `minPresenceAhead` | The `orphans` and `widows` CSS properties, 2 by default, and `breakAfter: 'avoid'` to keep an element with what follows. |
| `renderToBuffer`, `renderToStream`, `pdf().toBlob()` | `await render(…)` returns a `Uint8Array`. Wrap it in a `Blob` yourself. |
| `PDFViewer`, `PDFDownloadLink` | None. FlashPDF ships no viewer or download helper. |

Four differences cause most surprises:

- A `View` stacks its children in a flex column. A FlashPDF `div` is a block, which also stacks them. For a row, set `display: 'flex'`. FlashPDF follows CSS here, so a flex box is a row unless you set `flexDirection: 'column'`.
- Numbers in `style` are points in both libraries. FlashPDF also accepts `pt`, `px` (0.75 pt), `mm`, `cm`, `in`, `pc`, `Q`, `em`, and `rem`.
- `justifyContent`, `alignItems`, `flexWrap`, and `position` reject. Use `flex`, `width`, `gap`, and `textAlign`.

`Svg`, `Canvas`, `Note`, italic text, and lists have no equivalent.
