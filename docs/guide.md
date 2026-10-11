# FlashPDF guide

Setup for bundlers and browsers, fixes for common rejections, Tailwind v4, three recipes, and a map from react-pdf. The [README](../README.md) covers the API and [CSS.md](../CSS.md) covers the CSS subset.

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
| `src must be the image's PNG or JPEG bytes as a Uint8Array` | `img` does not take a URL, path, or `data:` URL. | Read or fetch the file and pass its bytes. |
| `unsupported element <ul>` | There are no lists. | One `<p>` per item that starts with `•`. |
| `unsupported style property: borderRadius`, `position`, `opacity`, `justifyContent`, `alignItems` | No equivalent exists. | Remove it. [CSS.md](../CSS.md) lists every limit. |
| `height applies only to <img>` | Block height comes from content. | Remove `height`. Space blocks with `margin` or `padding`. |
| `colSpan and rowSpan are not supported yet` | A table row needs one cell per column. | Split the cell. |
| `page tokens are only valid in a header or footer` | `PageNumber` and `TotalPages` only resolve there. | Move them into the `header` or `footer` option. |
| `href must be an absolute http, https, or mailto URL` | Links cannot be relative or `#anchors`. | Use a full URL. |
| `undefined CSS variable: --spacing` | A Tailwind build without `theme(inline)` keeps its values in a `:root` rule, which FlashPDF skips. | Import the theme as `theme(inline)`. See Tailwind v4 below. |
| `unsupported media query "(prefers-color-scheme: dark)"` | Only `print`, `screen`, and width conditions evaluate. | Drop the `dark:` or `motion-safe:` utility from the element. |

## Tailwind v4

Compile with the Tailwind CLI, then pass the CSS to `render`. Import the theme inline and only the utilities, so colour and spacing utilities carry their values and the preflight, whose `*` and `::before` rules FlashPDF skips, stays out. `examples/css-integrations/tailwind.mjs` builds and renders an invoice this way.

```css
@import 'tailwindcss/theme.css' theme(inline);
@import 'tailwindcss/utilities.css' source(none);
@source '../src/invoice.tsx';
```

```sh
tailwindcss -i invoice.css -o invoice.out.css --minify
```

Tailwind compiles spacing to `calc(0.25rem * 4)`, `px-4` to `padding-inline`, `w-1/2` to `calc(1 / 2 * 100%)`, and `border` to `border-style: var(--tw-border-style)` with an `@property` default of `solid`. FlashPDF evaluates the `calc()`, maps the logical properties to left, right, top, and bottom for left-to-right text, matches the escaped `.w-1\/2` to the class `w-1/2`, and reads the `@property` default. `@media` blocks apply when they match: `print` does, `screen` does not, and a width condition compares the page content width, which is the page width minus both margins. With A4 and the default 36 pt margin that is 523 pt, so `sm:` (480 pt) applies and `md:` (576 pt) does not unless `margin` is 9.5 pt or less.

| Utilities | Result |
| --- | --- |
| `p-*`, `px-*`, `py-*`, `m-*`, `mt-*`, and the other sides | Render. |
| `border`, `border-b`, `border-t`, `border-0`, `border-solid`, `border-none`, `border-gray-200` | Render. |
| `text-sm`, `text-lg`, and the other sizes, `leading-*`, `font-semibold`, `font-bold`, `text-right`, `text-gray-600`, `bg-gray-100`, `underline` | Render. |
| `w-1/2`, `w-32`, `w-[120px]` | Render on flex row children and first-row table cells, where the engine measures columns. Elsewhere `width applies only to a flex row child`. |
| `flex`, `flex-col`, `flex-row`, `flex-1`, `gap-4` | Render. `flex-1` only on a flex row child. Nothing shrinks, so two `w-1/2` columns with a `gap-4` reject with `InvalidLayout`. |
| `md:p-8`, `sm:p-4`, `print:p-0` | Render, when the media condition matches. |
| `-mt-4`, `mx-auto`, `w-auto` | Reject: `invalid length: -12pt` and `auto`, because margins cannot be negative or `auto`, and `invalid width: auto`. |
| `border-dashed`, `border-x`, `border-y` | Reject: dashed and dotted borders are not drawn, and `border-inline-*` and `border-block-*` are not mapped yet. |
| `rounded-*`, `shadow-*`, `opacity-*`, `bg-black/50`, `justify-*`, `items-*` | Reject: square corners, no shadows, no opacity (also in `/50` colours), no flex alignment. `stylesheet()` rejects `bg-black/50` as soon as it is in the sheet. |
| `hidden`, `grid`, `inline`, `h-8`, `min-w-0`, `max-w-md`, `size-4` | Reject: `display` is `block` or `flex`, block height comes from content, and `min-width` and `max-width` are not implemented. |
| `uppercase`, `italic`, `tracking-wide`, `truncate` | Reject: no `text-transform`, italic face, letter spacing, or `overflow`. |
| `dark:`, `motion-safe:`, `portrait:` | Reject with `unsupported media query`, only where the utility is used. |
| `hover:`, `first:`, `space-y-*` | Do nothing. Their selectors (`:hover`, `:first-child`, `:where(... > :not(:last-child))`) are outside the subset and skipped. |

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
| `Page size="A4"` | `pageFormat: 'A4'` or `'Letter'`. One size for the whole document. |
| `Page style={{ padding }}` | `margin`, in points, on all four sides. |
| A second `Page`, or `break` | `breakBefore: 'page'` on a direct child of the root. |
| `View` | `div`, `section`, `article`, `header`, `footer`, or `main`. |
| `Text` | `p`, `span`, `b`, `strong`, or `h1` to `h6`. A bare string inside a block also works. A nested `Text` becomes a `span` or `b` inside a `p`. |
| `Image src={url}` | `img src={bytes} alt="…"`. You read or fetch the PNG or JPEG bytes. |
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

`Svg`, `Canvas`, `Note`, italic text, and lists have no equivalent.
