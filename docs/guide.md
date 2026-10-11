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
| `PageOverflow: use document margins or split this decorated box` | A box with padding, a border, or a background cannot split across pages. A decorated root holding more than one page of content overflows. | Move the root's padding to the `margin` option. Drop its border and background, and decorate the cards inside it instead. |
| `PageOverflow: the table is taller than a page` or `the image is taller than the page` | An atomic block has to fit one page. | Remove `break-inside: avoid` from the table, or reduce the image height. |
| `Helvetica has no glyph for "−" (U+2212)` | The default font covers WinAnsi only. `Intl.NumberFormat` writes U+2212 for negatives in `nb-NO` and U+202F as the group separator in `fr-FR`. | Embed a TTF that has the glyph, or replace it: `text.replace(/−/g, '-').replace(/ /g, ' ')`. |
| `src must be the image's PNG, JPEG, or SVG bytes as a Uint8Array` | `img` does not take a URL, path, or `data:` URL. | Read or fetch the file and pass its bytes. |
| `SVG text is not supported` | A drawing has no text layout. | Convert the text to outlines in the editor, or put a `<p>` beside the drawing. |
| `gradients and patterns are not supported`, `clipPath is not supported`, `CSS inside <svg> is not supported` | A drawing has solid paint and no references or stylesheet. | Flatten the file with SVGO or the editor's export settings, or export a PNG. |
| `unsupported element <ul>` | There are no lists. | One `<p>` per item that starts with `•`. |
| `unsupported style property: borderRadius`, `position`, `opacity`, `justifyContent`, `alignItems` | No equivalent exists. | Remove it. [CSS.md](../CSS.md) lists every limit. |
| `height applies only to <img> and <svg>` | Block height comes from content. | Remove `height`. Space blocks with `margin` or `padding`. |
| `colSpan and rowSpan are not supported yet` | A table row needs one cell per column. | Split the cell. |
| `page tokens are only valid in a header or footer` | `PageNumber` and `TotalPages` only resolve there. | Move them into the `header` or `footer` option. |
| `href must be an absolute http, https, or mailto URL` | Links cannot be relative or `#anchors`. | Use a full URL. |

## Recipes

### Totals table

Give the totals table the same amount column width as the items table, so the numbers line up. The first column has no width and takes the rest. Keep the grand total out of `tfoot`, because `tfoot` rows repeat on every page.

```tsx
const money = new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' });
const format = (cents: number) => money.format(cents / 100);
const amount = { width: '90pt', textAlign: 'right' } as const;

const subtotal = items.reduce((sum, item) => sum + item.cents, 0);
const tax = Math.round(subtotal * 0.25);

<table style={{ marginTop: 8, breakInside: 'avoid' }}>
  <tbody>
    <tr>
      <td style={{ textAlign: 'right' }}>Subtotal</td>
      <td style={amount}>{format(subtotal)}</td>
    </tr>
    <tr>
      <td style={{ textAlign: 'right' }}>VAT 25%</td>
      <td style={amount}>{format(tax)}</td>
    </tr>
    <tr>
      <th style={{ textAlign: 'right' }}>Total</th>
      <th style={amount}>{format(subtotal + tax)}</th>
    </tr>
  </tbody>
</table>
```

Add amounts as integer cents and round once. `breakInside: 'avoid'` keeps the three rows on one page.

### Page X of Y footer

```tsx
import { PageNumber, TotalPages, render } from '@pettersen3008/flashpdf';

const pdf = await render(<Invoice />, {
  footer: (
    <footer style={{ fontSize: '9pt', textAlign: 'center' }}>
      Page <PageNumber /> of <TotalPages />
    </footer>
  ),
});
```

A header or footer is one styled text line that repeats on every page and reserves its height. Footer text takes no links or decoration.

### EPC payment QR code (GiroCode)

European banking apps read a SEPA credit transfer from an EPC QR code, also called GiroCode. The payload is plain text with one field per line, as the EPC069-12 guideline defines it:

```tsx
import { QRCode } from '@pettersen3008/flashpdf';

const epc = [
  'BCD', // service tag
  '002', // version; 002 makes the BIC optional
  '1', // character set: UTF-8
  'SCT', // SEPA credit transfer
  'BFSWDE33', // BIC, or an empty line
  'Acme GmbH', // beneficiary, at most 70 characters
  'DE71110220330123456789', // IBAN
  'EUR1234.50', // amount
  '', // purpose code
  'RF18539007547034', // creditor reference; leave it empty and add a line of text instead to send a message
].join('\n');

<QRCode value={epc} level="M" alt="Pay EUR 1,234.50 to Acme GmbH" style={{ width: 113 }} />
```

The guideline asks for error correction level M and at most 331 bytes. FlashPDF encodes the text as given: it does not check the IBAN, BIC, amount, reference, or field order, so validate the payload with your payment library and scan a test print with a banking app.

### Swiss QR-bill QR code

The Swiss QR code holds the payment part of a QR-bill. Lines end with LF or CR LF, and the fields follow the Swiss Implementation Guidelines:

```tsx
import { QRCode } from '@pettersen3008/flashpdf';

const swiss = [
  'SPC', '0200', '1', // QR type, version, UTF-8
  'CH4431999123000889012', // QR-IBAN or IBAN
  'S', 'Robert Schneider AG', 'Rue du Lac', '1268', '2501', 'Biel', 'CH', // creditor, structured address
  '', '', '', '', '', '', '', // ultimate creditor, reserved
  '1949.75', 'CHF',
  'S', 'Pia-Maria Rutschmann-Schnyder', 'Grosse Marktgasse', '28', '9400', 'Rorschach', 'CH', // debtor
  'QRR', '210000000003139471430009017', // reference type and reference
  'Order dated 18.06.2020', // unstructured message
  'EPD', // trailer
].join('\n');

// The 7 mm Swiss cross on a 46 mm symbol, in the 100 by 100 box that QRCode children draw in.
<QRCode value={swiss} level="M" quietZone={0} alt="Swiss QR code" style={{ width: 130.4, margin: 14.2 }}>
  <rect x={42.39} y={42.39} width={15.22} height={15.22} fill="#fff" />
  <rect x={43.48} y={43.48} width={13.04} height={13.04} fill="#000" />
  <rect x={45.92} y={48.78} width={8.16} height={2.44} fill="#fff" />
  <rect x={48.78} y={45.92} width={2.44} height={8.16} fill="#fff" />
</QRCode>
```

The guidelines ask for error correction level M, a 46 mm square symbol (130.4 pt), a 5 mm quiet zone (14.2 pt, here the margin), and a 7 mm Swiss cross in the centre. The cross above follows the flag's proportions; draw SIX's official artwork instead if your bank or printer requires it. The cross covers the centre alignment pattern, as the standard intends: zxing-cpp reads such a code, and zbar 0.23 does not. FlashPDF does not check the payload against the QR-bill rules or the reference checksum, and the printed payment part with its fields and layout is yours to build.

## Migrating from react-pdf

| react-pdf | FlashPDF |
| --- | --- |
| `Document` | No wrapper. Pass the root element, or an array of roots, to `render`. `metadata` sets title, author, subject, keywords, and language. |
| `Page size="A4"` | `pageFormat: 'A4'` or `'Letter'`. One size for the whole document. |
| `Page style={{ padding }}` | `margin`, in points, on all four sides. |
| A second `Page`, or `break` | `breakBefore: 'page'` on a direct child of the root. |
| `View` | `div`, `section`, `article`, `header`, `footer`, or `main`. |
| `Text` | `p`, `span`, `b`, `strong`, or `h1` to `h6`. A bare string inside a block also works. A nested `Text` becomes a `span` or `b` inside a `p`. |
| `Image src={url}` | `img src={bytes} alt="…"`. You read or fetch the PNG, JPEG, or SVG bytes. |
| `Svg`, `Path`, `Rect`, `G`, … | `svg`, `path`, `rect`, `g`, … in lowercase with the same attributes. Text, gradients, and clip paths reject. |
| `Link src` | `a href`, absolute `http:`, `https:`, or `mailto:`. |
| `StyleSheet.create` | A `style` object, or CSS strings in `stylesheets`. |
| `Font.register` | `fonts: [{ family, regular, bold }]` with TTF bytes. Regular and bold only. |
| `fixed` header or footer | The `header` and `footer` options, one text line each. |
| `render={({ pageNumber, totalPages }) => …}` | `<PageNumber />` and `<TotalPages />` inside `header` or `footer`. |
| `wrap={false}` | `breakInside: 'avoid'`. |
| `renderToBuffer`, `renderToStream`, `pdf().toBlob()` | `await render(…)` returns a `Uint8Array`. Wrap it in a `Blob` yourself. |
| `PDFViewer`, `PDFDownloadLink` | None. FlashPDF ships no viewer or download helper. |

Three differences cause most surprises:

- A `View` stacks its children in a flex column. A FlashPDF `div` is a block, which also stacks them. For a row, set `display: 'flex'`. FlashPDF follows CSS here, so a flex box is a row unless you set `flexDirection: 'column'`.
- Numbers in `style` are points in both libraries. FlashPDF also accepts `pt`, `px` (0.75 pt), `em`, and `rem`.
- `justifyContent`, `alignItems`, `flexWrap`, and `position` reject. Use `flex`, `width`, `gap`, and `textAlign`.

`Canvas`, `Note`, italic text, and lists have no equivalent.
