# FlashPDF guide

Setup for bundlers and browsers, fixes for common rejections, Tailwind v4, four recipes, and a map from react-pdf. The [README](../README.md) covers the API and [CSS.md](../CSS.md) covers the CSS subset.

## Bundlers and runtimes

The browser entry loads the WASM with `new URL('../wasm/flashpdf_wasm_bg.wasm', import.meta.url)`. A bundler that understands that pattern emits the file as an asset. Serve it as `application/wasm`: browsers compile it while it streams only with that type, and otherwise FlashPDF falls back to a slower path and logs a warning.

**Vite.** No plugin and no `optimizeDeps` setting. The dev server and `vite build` both work, and `tests/e2e.mjs` builds with Vite on every run.

**webpack 5.** No loader and no `experiments.asyncWebAssembly`. webpack emits the WASM as a hashed asset next to the bundle. Set `output.publicPath` to where you serve that asset, as for any webpack asset.

**Next.js 16.** A client component needs no configuration. Server code, such as a route handler on the Node.js runtime, also works with the default Turbopack build. With `next build --webpack`, the server bundle rewrites the WASM path and `render` fails with `ERR_INVALID_URL`. Keep the package out of that bundle:

```js
// next.config.mjs
export default { serverExternalPackages: ["@pettersen3008/flashpdf"] };
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

| Message                                                                                                                               | Cause                                                                                                                                                                           | Fix                                                                                                                                                                 |
| ------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `fontStyle: oblique 10deg needs a font with a slnt axis`                                                                              | The face has no `slnt` axis, and FlashPDF slants an upright face only by 14 degrees.                                                                                            | Use `fontStyle: 'oblique'` or `'italic'`, or register a variable font with a `slnt` axis.                                                                                              |
| `invalid letterSpacing: 5%`                                                                                                           | Spacing takes a length, not a percentage.                                                                                                                                       | Use `em`, such as `0.05em`.                                                                                                                                         |
| `text-decoration: wavy is not supported; lines are solid`                                                                             | Decoration lines are solid.                                                                                                                                                     | Drop the style keyword.                                                                                                                                             |
| `PageOverflow: the table is taller than a page` or `the image is taller than the page`                                                | An atomic block has to fit one page.                                                                                                                                            | Remove `break-inside: avoid` from the table, or reduce the image height.                                                                                            |
| `src must be the image's PNG, JPEG, or SVG bytes as a Uint8Array`                                                                     | `img` does not take a URL, path, or `data:` URL.                                                                                                                                | Read or fetch the file and pass its bytes.                                                                                                                          |
| `SVG text is not supported`                                                                                                           | A drawing has no text layout.                                                                                                                                                   | Convert the text to outlines in the editor, or put a `<p>` beside the drawing.                                                                                      |
| `gradients and patterns are not supported`, `clipPath is not supported`, `CSS inside <svg> is not supported`                          | A drawing has solid paint and no references or stylesheet.                                                                                                                      | Flatten the file with SVGO or the editor's export settings, or export a PNG.                                                                                        |
| `border-radius needs one width, colour, and style on all four borders, or none`                                                       | A rounded border is one stroke around the curve, so differing sides have no mitre to meet at.                                                                                   | Give all four sides the same border, or round a box that has only a background and draw the rule on a child.                                                        |
| `border-radius does not clip <img>` or `<svg>`                                                                                        | The picture stays rectangular.                                                                                                                                                  | Remove the radius, or crop the picture in an editor.                                                                                                                |
| `unsupported style property: boxShadow`, `outline`, `borderImage`, `backgroundImage`, or `unsupported background: linear-gradient(…)` | Shadows, outlines, border images, and gradients are not drawn.                                                                                                                  | Use a border or a solid `background-color`.                                                                                                                         |
| `the built-in font has no glyph for "−" (U+2212)`                                                                                     | The built-in fonts, Helvetica, Times, and Courier, cover WinAnsi only. `Intl.NumberFormat` writes U+2212 for negatives in `nb-NO` and U+202F as the group separator in `fr-FR`. | Register any TTF that has the glyph, which the text then falls back to, or replace it: `text.replace(/−/g, '-').replace(/ /g, ' ')`.                                                                        |
| `unregistered font family: Inter`                                                                                                     | No entry of the `fontFamily` list is a registered family or a built-in name. An entry that matches nothing is skipped for a later one, so only a list with no match rejects.    | Register the font with `fonts` and spell `fontFamily` exactly as its `family`, which is case-sensitive. Or end the list with `serif`, `sans-serif`, or `monospace`. |
| `<li> belongs in a <ul> or <ol>` or `<ul> accepts only <li>`                                                                          | An item sits outside a list, or a list holds something other than items.                                                                                                        | Wrap each `li` directly in a `ul` or `ol`.                                                                                                                          |
| `LineTooWide: a line of white-space: pre or nowrap text is wider than its container`                                                  | A `pre` line, or text under `white-space: nowrap`, does not fit the page. FlashPDF never clips or breaks it silently.                                                           | Shorten the line, reduce `fontSize`, or use `white-space: pre-wrap` to wrap it.                                                                                     |
| `page tokens are only valid in a header or footer`                                                                                    | `PageNumber` and `TotalPages` only resolve there.                                                                                                                               | Move them into the `header` or `footer` option.                                                                                                                     |
| `href must be an absolute http, https, or mailto URL`                                                                                 | Links cannot be relative.                                                                                                                                                       | Use a full URL, or `#id` for an element in this document.                                                                                                           |
| `link to #terms: no element has id="terms"`                                                                                           | The `#id` names no element in the document body.                                                                                                                                | Add the `id`, or fix the spelling. The match is exact and case sensitive.                                                                                           |
| `link to #terms: id="terms" is on more than one element`                                                                              | A destination needs one element.                                                                                                                                                | Give each element its own `id`.                                                                                                                                     |
| `link to #terms: its element cannot hold a destination`                                                                               | The id is on an inline `span`, `b`, `strong`, or `a`, a `br`, or a table row.                                                                                                   | Move the `id` to the paragraph, a heading, a block, or a table cell.                                                                                                |
| `breakBefore: left is not supported (pages have no sides; use page)`                                                                  | `left`, `right`, `recto`, and `verso` pick a page side, and pages have none.                                                                                                    | Use `page`.                                                                                                                                                         |
| `invalid orphans: 0 (use a whole number of lines from 1)`                                                                             | `orphans` and `widows` count lines.                                                                                                                                             | Use 1 to break wherever the page ends.                                                                                                                              |
| `nested break-before is not implemented`                                                                                              | Flex rows, table rows, and `break-inside: avoid` groups never break inside.                                                                                                     | Put the break on the flex row, the table, or the group.                                                                                                             |
| `undefined CSS variable: --spacing`                                                                                                   | A Tailwind build without `theme(inline)` keeps its values in a `:root` rule inside `@layer theme`, which FlashPDF skips with every `@layer` block.                              | Import the theme as `theme(inline)`. See Tailwind v4 below.                                                                                                         |
| `unsupported media query "(prefers-color-scheme: dark)"`                                                                              | Only `print`, `screen`, and width conditions evaluate.                                                                                                                          | Drop the `dark:` or `motion-safe:` utility from the element.                                                                                                        |
| `unsupported CSS selector: attribute selectors`, `:has()`, `:lang()`                                                                  | The selector could match an element, and FlashPDF cannot evaluate it. `render` rejects it even if no element matches.                                                              | Select a class or id instead. Selectors that can never match in a PDF, such as `:hover` and `::before`, are skipped without an error.                               |

## Tailwind v4

Compile with the Tailwind CLI, then pass the CSS to `render`. Import the theme inline and only the utilities, so colour and spacing utilities carry their values and the browser preflight stays out. `examples/css-integrations/tailwind.mjs` builds and renders an invoice this way.

```css
@import "tailwindcss/theme.css" theme(inline);
@import "tailwindcss/utilities.css" source(none);
@source '../src/invoice.tsx';
```

```sh
tailwindcss -i invoice.css -o invoice.out.css --minify
```

Tailwind compiles spacing to `calc(0.25rem * 4)`, `px-4` to `padding-inline`, `w-1/2` to `calc(1 / 2 * 100%)`, and `border` to `border-style: var(--tw-border-style)` with an `@property` default of `solid`. FlashPDF evaluates the `calc()`, maps the logical properties to left, right, top, and bottom for left-to-right text, matches the escaped `.w-1\/2` to the class `w-1/2`, and reads the `@property` default. `@media` blocks apply when they match: `print` does, `screen` does not, and a width condition compares the page content width, which is the page width minus both margins. With A4 and the default 36 pt margin that is 523 pt, so `sm:` (480 pt) applies and `md:` (576 pt) does not unless `margin` is 9.5 pt or less.

| Utilities                                                                                                                                       | Result                                                                                                                                                                                                                                                                                                                               |
| ----------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `p-*`, `px-*`, `py-*`, `m-*`, `mt-*`, and the other sides                                                                                       | Render.                                                                                                                                                                                                                                                                                                                              |
| `border`, `border-b`, `border-t`, `border-0`, `border-solid`, `border-none`, `border-gray-200`                                                  | Render.                                                                                                                                                                                                                                                                                                                              |
| `text-sm`, `text-lg`, and the other sizes, `leading-*`, `font-semibold`, `font-bold`, `text-right`, `text-gray-600`, `bg-gray-100`, `underline` | Render.                                                                                                                                                                                                                                                                                                                              |
| `w-1/2`, `w-32`, `w-[120px]`, `min-w-0`, `max-w-md`, `h-8`, `size-4`                                                                            | Render on supported block and flex elements. A fixed height rejects content that does not fit.                                                                                                                                                                                                                                       |
| `flex`, `flex-col`, `flex-row`, `flex-1`, `gap-4`, `justify-*`, `items-*`, `self-*`                                                             | Render within the [flex limits](../CSS.md#supported-subset). Row items can grow and shrink; column distribution needs a fixed height or minimum height.                                                                                                                                                                              |
| `md:p-8`, `sm:p-4`, `print:p-0`                                                                                                                 | Render, when the media condition matches.                                                                                                                                                                                                                                                                                            |
| `mx-auto`                                                                                                                                       | Centres a block narrower than its container. Negative margins such as `-mt-4` reject.                                                                                                                                                                                                                                                |
| `border-dashed`, `border-dotted`                                                                                                                | Render.                                                                                                                                                                                                                                                                                                                              |
| `rounded-*`, `opacity-*`                                                                                                                        | Render. Rounded borders need the same width, colour, and style on all four sides. Logical corner utilities such as `rounded-s-*` reject.                                                                                                                                                                                             |
| `shadow-*`, `outline-*`, `ring-*`, `bg-gradient-*`, `bg-black/50`                                                                               | Reject: shadows, outlines, gradients, and alpha colour syntax are unsupported. Use element opacity for transparency.                                                                                                                                                                                                                 |
| `hidden`, `grid`, `inline`                                                                                                                      | Reject: explicit `display` values are `block` or `flex`.                                                                                                                                                                                                                                                                             |
| `uppercase`, `italic`, `tracking-wide`                                                                                                          | Render. `truncate` rejects because text overflow and clipping are unsupported.                                                                                                                                                                                                                                                       |
| `dark:`, `motion-safe:`, `portrait:`                                                                                                            | Reject with `unsupported media query`, only where the utility is used.                                                                                                                                                                                                                                                               |
| `first:`, `last:`, `only:`, `odd:`, `even:`, `empty:`, `nth-*:`, `*:`, `group-first:`, `space-y-*`, `divide-y`, `divide-gray-200`               | Render. They compile to `:first-child`, `:nth-child(odd)`, `:is(.x > *)`, and `:where(.space-y-4 > :not(:last-child))`. Position counts the element siblings: text, `null`, and fragments do not count, so `first:` on a `<tr>` is the first row of its `<tbody>`. `space-y-*` and `divide-y` style the children, not the container. |
| `hover:`, `focus:`, `active:`, `disabled:`, `checked:`, `group-hover:`, `before:`, `placeholder:`, `selection:`                                 | Do nothing, quietly. A PDF has no pointer, focus, or form state and no generated content, so these selectors never match.                                                                                                                                                                                                            |
| `data-*:`, `aria-*:`, `has-*:`, `open:`, `rtl:`, `ltr:`                                                                                         | Reject with the selector and reason: attribute selectors, `:has()`, and `:dir()` are not implemented.                                                                                                                                                                                                                                |

## Recipes

### Invoices from HTML templates

A template engine that already writes HTML, such as Handlebars or a server-side string template, can hand its output to `render` as a string. Escape the values it fills in as HTML, since the parser rejects a bare `&` or `<`. Pass the logo in `images` and name it in `<img src>`, and put shared CSS in a `<style>` element or in `stylesheets`. See [HTML](../packages/flashpdf/README.md#html) for what the parser accepts.

### Totals table

Put the totals in the items table, with each label spanning the columns left of the amounts, so the numbers line up with the amount column. Keep the grand total out of `tfoot`, because `tfoot` rows repeat on every page.

```tsx
const money = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });
const format = (cents: number) => money.format(cents / 100);
const right = { textAlign: "right" } as const;

const subtotal = items.reduce((sum, item) => sum + item.cents, 0);
const tax = Math.round(subtotal * 0.25);

<table>
  <thead>
    <tr>
      <th>Description</th>
      <th style={{ width: "40pt", textAlign: "right" }}>Qty</th>
      <th style={{ width: "90pt", textAlign: "right" }}>Amount</th>
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
  <tbody style={{ breakInside: "avoid" }}>
    <tr>
      <td colSpan={2} style={right}>
        Subtotal
      </td>
      <td style={right}>{format(subtotal)}</td>
    </tr>
    <tr>
      <td colSpan={2} style={right}>
        VAT 25%
      </td>
      <td style={right}>{format(tax)}</td>
    </tr>
    <tr>
      <th colSpan={2} style={right}>
        Total
      </th>
      <th style={right}>{format(subtotal + tax)}</th>
    </tr>
  </tbody>
</table>;
```

Add amounts as integer cents and round once. `breakInside: 'avoid'` keeps the three rows on one page, and `breakBefore: 'avoid'` keeps them with the last item rows, so the totals never start a page alone.

### Page breaks

Pages break as in CSS: between blocks, between table rows, and between the lines of a paragraph, keeping at least two lines on each side (`orphans` and `widows`). A box with a border or background that crosses the page end splits there, drawing its top edge on the first page and its bottom edge on the last.

```tsx
<section style={{ breakBefore: "page" }}>
  {" "}
  {/* start on a new page */}
  <h2>Payment terms</h2> {/* stays with the next lines */}
  <p style={{ orphans: 3, widows: 3 }}>…</p> {/* three lines on each side of a break */}
  <div style={{ breakInside: "avoid", border: "1pt solid #d0d5dd", padding: 8 }}>
    Bank details, kept on one page
  </div>
</section>
```

Headings `h1` to `h6` default to `breakAfter: 'avoid'`, so a heading moves to the next page when the first two lines of its paragraph do not fit below it. That leaves space at the bottom of the page on purpose; set `breakAfter: 'auto'` on the heading to fill it instead. `breakAfter: 'avoid'` on a `tr` keeps it with the next row, under the repeated header.

When avoid rules leave no break point on a page, FlashPDF breaks anyway rather than lose content, first ignoring `orphans` and `widows`, then the avoid rules. It holds back at most one page of content for these decisions, so a run of avoided breaks taller than a page breaks where the page ends. A `breakInside: 'avoid'` box taller than a page rejects with `PageOverflow`.

### Page X of Y footer and a first-page letterhead

```tsx
import { PageNumber, TotalPages, render } from "@pettersen3008/flashpdf";

const pdf = await render(<Invoice />, {
  firstPageHeader: (
    <header style={{ backgroundColor: "#eef3fb", padding: "10pt", marginBottom: "12pt" }}>
      <img src={logo} alt="Acme Supply AS" style={{ width: 120 }} />
    </header>
  ),
  header: <header style={{ fontSize: "9pt", textAlign: "right" }}>Invoice 2026-0042</header>,
  footer: (
    <footer style={{ display: "flex", gap: "12pt", fontSize: "8pt", borderTop: "1px solid #ccc" }}>
      <p>
        Acme Supply AS
        <br />
        Storgata 1, 0150 Oslo
      </p>
      <p>
        <b>Bank</b>
        <br />
        IBAN NO93 8601 1117 947
      </p>
      <p style={{ textAlign: "right" }}>
        Org. no. 912 345 678
        <br />
        Page <PageNumber /> of <TotalPages />
      </p>
    </footer>
  ),
});
```

A header or footer takes anything the body takes and paints on every page, as an artifact in tagged mode. `firstPageHeader` and `firstPageFooter` replace them on page one; pass `null` to leave page one without one. Each page reserves the height of the header and footer it paints, so the body starts lower under a tall letterhead.

The height is measured once, before the body, with every page number counted as five digits. Each page then lays out its header and footer again with its real numbers, so alignment fits the actual digits. Give a page number room for five digits on its line, or the header or footer reserves an extra line it does not paint.

### EPC payment QR code (GiroCode)

European banking apps read a SEPA credit transfer from an EPC QR code, also called GiroCode. The payload is plain text with one field per line, as the EPC069-12 guideline defines it:

```tsx
import { QRCode } from "@pettersen3008/flashpdf";

const epc = [
  "BCD", // service tag
  "002", // version; 002 makes the BIC optional
  "1", // character set: UTF-8
  "SCT", // SEPA credit transfer
  "BFSWDE33", // BIC, or an empty line
  "Acme GmbH", // beneficiary, at most 70 characters
  "DE71110220330123456789", // IBAN
  "EUR1234.50", // amount
  "", // purpose code
  "RF18539007547034", // creditor reference; leave it empty and add a line of text instead to send a message
].join("\n");

<QRCode value={epc} level="M" alt="Pay EUR 1,234.50 to Acme GmbH" style={{ width: 113 }} />;
```

The guideline asks for error correction level M and at most 331 bytes. FlashPDF encodes the text as given: it does not check the IBAN, BIC, amount, reference, or field order, so validate the payload with your payment library and scan a test print with a banking app.

### Swiss QR-bill QR code

The Swiss QR code holds the payment part of a QR-bill. Lines end with LF or CR LF, and the fields follow the Swiss Implementation Guidelines:

```tsx
import { QRCode } from "@pettersen3008/flashpdf";

const swiss = [
  "SPC",
  "0200",
  "1", // QR type, version, UTF-8
  "CH4431999123000889012", // QR-IBAN or IBAN
  "S",
  "Robert Schneider AG",
  "Rue du Lac",
  "1268",
  "2501",
  "Biel",
  "CH", // creditor, structured address
  "",
  "",
  "",
  "",
  "",
  "",
  "", // ultimate creditor, reserved
  "1949.75",
  "CHF",
  "S",
  "Pia-Maria Rutschmann-Schnyder",
  "Grosse Marktgasse",
  "28",
  "9400",
  "Rorschach",
  "CH", // debtor
  "QRR",
  "210000000003139471430009017", // reference type and reference
  "Order dated 18.06.2020", // unstructured message
  "EPD", // trailer
].join("\n");

// The 7 mm Swiss cross on a 46 mm symbol, in the 100 by 100 box that QRCode children draw in.
<QRCode
  value={swiss}
  level="M"
  quietZone={0}
  alt="Swiss QR code"
  style={{ width: 130.4, margin: 14.2 }}
>
  <rect x={42.39} y={42.39} width={15.22} height={15.22} fill="#fff" />
  <rect x={43.48} y={43.48} width={13.04} height={13.04} fill="#000" />
  <rect x={45.92} y={48.78} width={8.16} height={2.44} fill="#fff" />
  <rect x={48.78} y={45.92} width={2.44} height={8.16} fill="#fff" />
</QRCode>;
```

The guidelines ask for error correction level M, a 46 mm square symbol (130.4 pt), a 5 mm quiet zone (14.2 pt, here the margin), and a 7 mm Swiss cross in the centre. The cross above follows the flag's proportions; draw SIX's official artwork instead if your bank or printer requires it. The cross covers the centre alignment pattern, as the standard intends: zxing-cpp reads such a code, and zbar 0.23 does not. FlashPDF does not check the payload against the QR-bill rules or the reference checksum, and the printed payment part with its fields and layout is yours to build.

## Migrating from react-pdf

| react-pdf                                                    | FlashPDF                                                                                                                                                                                                                                       |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Document`                                                   | No wrapper. Pass the root element, or an array of roots, to `render`. `metadata` sets title, author, subject, keywords, and language.                                                                                                          |
| `Page size="A4" orientation="landscape"`                     | `pageFormat: 'A4'` (also A3, A5, Letter, Legal, or `{ width, height }`) and `orientation: 'landscape'`, or `@page { size: A4 landscape }`. One size for the whole document.                                                                    |
| `Page style={{ padding }}`                                   | `margin`: one to four lengths, such as `'20mm 15mm'`, or `{ top, right, bottom, left }`, or `@page { margin }`.                                                                                                                                |
| A second `Page`, or `break`                                  | `breakBefore: 'page'` on a direct child of the root.                                                                                                                                                                                           |
| `View`                                                       | `div`, `section`, `article`, `header`, `footer`, or `main`.                                                                                                                                                                                    |
| `Text`                                                       | `p`, `span`, `b`, `strong`, `em`, `i`, `code`, `sub`, `sup`, or `h1` to `h6`. A bare string inside a block also works. A nested `Text` becomes a `span`, `b`, or `em` inside a `p`.                                                            |
| `Image src={url}`                                            | `img src={bytes} alt="…"`. You read or fetch the PNG, JPEG, or SVG bytes.                                                                                                                                                                      |
| `Svg`, `Path`, `Rect`, `G`, …                                | `svg`, `path`, `rect`, `g`, … in lowercase with the same attributes. Text, gradients, and clip paths reject.                                                                                                                                   |
| `Link src`                                                   | `a href`, absolute `http:`, `https:`, or `mailto:`.                                                                                                                                                                                            |
| `borderRadius`, `borderStyle`, `opacity`                     | The same names with CSS meanings. A radius needs the same border on all four sides, and opacity applies per element rather than to the group.                                                                                                  |
| `id` with `Link src="#id"`                                   | `id` with `a href="#id"`. The link lands on the page where the element does.                                                                                                                                                                   |
| `bookmark`                                                   | `outline: true` bookmarks `h1` to `h6`. `data-bookmark="Title"` renames one and `data-bookmark={false}` skips it.                                                                                                                              |
| `StyleSheet.create`                                          | A `style` object, or CSS strings in `stylesheets`.                                                                                                                                                                                             |
| `Font.register`                                              | `fonts: [{ family, regular, bold, italic, boldItalic }]` with TTF bytes. Only `regular` is required.                                                                                                                                           |
| `fontFamily: 'Times-Roman'`, `'Courier'`, `'Helvetica-Bold'` | The built-in families are `Helvetica`, `Times`, and `Courier`, or `sans-serif`, `serif`, and `monospace`. Choose the face with `fontWeight` and `fontStyle`. Other react-pdf standard names, such as `Times-Roman`, are not family names here. |
| `fixed` header or footer                                     | The `header` and `footer` options, which take any element tree. `firstPageHeader` and `firstPageFooter` cover a different first page.                                                                                                          |
| `render={({ pageNumber, totalPages }) => …}`                 | `<PageNumber />` and `<TotalPages />` anywhere inside a header or footer option.                                                                                                                                                               |
| `wrap={false}`                                               | `breakInside: 'avoid'`.                                                                                                                                                                                                                        |
| `orphans`, `widows`, `minPresenceAhead`                      | The `orphans` and `widows` CSS properties, 2 by default, and `breakAfter: 'avoid'` to keep an element with what follows.                                                                                                                       |
| `renderToBuffer`, `renderToStream`, `pdf().toBlob()`         | `await render(…)` returns a `Uint8Array`. Wrap it in a `Blob` yourself.                                                                                                                                                                        |
| `PDFViewer`, `PDFDownloadLink`                               | None. FlashPDF ships no viewer or download helper.                                                                                                                                                                                             |

Four differences cause most surprises:

- A `View` stacks its children in a flex column. A FlashPDF `div` is a block, which also stacks them. For a row, set `display: 'flex'`. FlashPDF follows CSS here, so a flex box is a row unless you set `flexDirection: 'column'`.
- react-pdf adds vertical margins. FlashPDF collapses adjoining ones in a block, as a browser does, so `marginBottom: 12` above `marginTop: 8` leaves 12 points, not 20. To keep react-pdf's spacing, give the parent `display: 'flex'` and `flexDirection: 'column'`: flex items never collapse.
- Numbers in `style` are points in both libraries. FlashPDF also accepts `pt`, `px` (0.75 pt), `mm`, `cm`, `in`, `pc`, `Q`, `em`, and `rem`.
- `justifyContent` and `alignItems` work per flex line, and `flexWrap: "wrap"` wraps a row into lines that break across pages. `wrap-reverse` rejects. See [CSS.md](../CSS.md) for sizing and alignment limits.

`Canvas` and `Note` have no equivalent. Use `ul`, `ol`, and `li` for lists; they number themselves and keep each marker with its first line.
