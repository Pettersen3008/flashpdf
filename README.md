# FlashPDF

Render invoices and reports from native JSX and a static CSS subset. A Rust layout engine compiled to WebAssembly produces the same PDF bytes in browsers, Node, and Bun. FlashPDF runs without React or a browser DOM; it does not lay out arbitrary HTML or CSS.

## Quickstart

```tsx
/** @jsxImportSource @pettersen3008/flashpdf */
import { render } from '@pettersen3008/flashpdf';

function Invoice({ total }: { total: string }) {
  return (
    <main style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <h1>Invoice</h1>
      <p>Due in <b>14 days</b></p>
      <div style={{ display: 'flex', flexDirection: 'row' }}>
        <span style={{ flex: 1 }}>Consulting</span>
        <span style={{ width: '90pt', textAlign: 'right' }}>{total}</span>
      </div>
    </main>
  );
}

const pdf = await render(<Invoice total="EUR 1200.00" />, { pageFormat: 'A4', margin: 36 });
```

## Playground

The playground at https://pettersen3008.github.io/flashpdf/ renders a TSX template and a stylesheet to a PDF in the browser, with FlashPDF's validation errors shown in full. Run it locally with `pnpm playground`, share a template through the URL hash, and upload a `.ttf` to test embedded fonts. Export `{ document, options }` from the TSX editor to pass render options; the CSS editor and font uploader supply the default `stylesheets` and `fonts`. It deploys to GitHub Pages from `main` after CI passes there, and lives in [`packages/playground`](./packages/playground).

## Tables

A `<table>` splits between rows and repeats its `<thead>` on every page. Column widths come from the first row's cells, cells stretch to the row height, and `th` is bold by default.

```tsx
<table>
  <thead>
    <tr>
      <th>Description</th>
      <th style={{ width: '90pt', textAlign: 'right' }}>Amount</th>
    </tr>
  </thead>
  <tbody>
    {items.map((item) => (
      <tr key={item.id}>
        <td style={{ borderBottom: '0.5pt solid #ddd' }}>{item.description}</td>
        <td style={{ borderBottom: '0.5pt solid #ddd', textAlign: 'right' }}>{item.amount}</td>
      </tr>
    ))}
  </tbody>
</table>
```

`colSpan`, `rowSpan`, `vertical-align`, and `border-collapse` reject with a reason; see [CSS.md](./CSS.md).

## Images and links

`img` takes the PNG or JPEG bytes as a `Uint8Array`, like fonts, and `a` takes an absolute `http:`, `https:`, or `mailto:` URL. In tagged mode, a nonempty `alt` becomes the figure description; an empty `alt` marks the image as decoration.

```tsx
const logo = new Uint8Array(await readFile('logo.png'));
<a href="https://example.com/invoices/42">
  <img src={logo} alt="Acme Supply Co." style={{ width: 120 }} />
</a>
<p>Pay online at <a href="https://example.com/pay">the portal</a>.</p>
```

PNG at 8 bits per channel (greyscale, RGB, palette, and alpha variants) and baseline or progressive JPEG (greyscale, RGB, CMYK) embed without re-encoding. Other depths, interlaced PNGs, and other formats reject naming the feature; see [CSS.md](./CSS.md).

## Install

Configure npm for the GitHub Packages scope in your project `.npmrc` and set `NODE_AUTH_TOKEN` to a classic GitHub personal access token with `read:packages`:

```ini
@pettersen3008:registry=https://npm.pkg.github.com
//npm.pkg.github.com/:_authToken=${NODE_AUTH_TOKEN}
```

Then install:

```sh
npm install @pettersen3008/flashpdf
pnpm add @pettersen3008/flashpdf
bun add @pettersen3008/flashpdf
```

FlashPDF owns this JSX runtime, so it has no framework dependency. Add `jsxImportSource: "@pettersen3008/flashpdf"` to a template-only `tsconfig`, or use the file pragma above in an app that also uses React. FlashPDF also accepts ordinary React host-element trees. It resolves pure function components and fragments without mounting a DOM. Effects, browser layout, and hook state do not belong in a static PDF template.

`render` returns a `Uint8Array`. FlashPDF ships no viewer, download, or upload helper, so you own what happens next:

```ts
const url = URL.createObjectURL(new Blob([pdf], { type: 'application/pdf' }));
```

## Styling with CSS

Compile stylesheets with the consuming app's build tool and pass the resulting string to `render`:

```tsx
import css from './invoice.css?inline';
import { render } from '@pettersen3008/flashpdf';

const pdf = await render(<main className="invoice">…</main>, { stylesheets: [css] });
```

CSS Modules, minification, and syntax transforms belong to the frontend build. Lightning CSS works well as a Vite, webpack, or Rollup plugin. FlashPDF only validates the subset it can render.

Runnable Tailwind, StyleX, and styled-components build-step examples live in [`packages/flashpdf/examples/css-integrations`](./packages/flashpdf/examples/css-integrations). They compile or extract static CSS, then pass that string through `render({ stylesheets })`.

FlashPDF rejects any declaration it cannot honour on an element it renders, naming the property, selector, source position, and element path. Unused rules, at-rules, and unsupported selectors are skipped.

## API

| Export | Signature | Notes |
| --- | --- | --- |
| `render` | `(element: Element \| Iterable<Element>, options?: RenderOptions) => Promise<Uint8Array>` | The root component must resolve to one or more host elements. Loads the WASM module once per process. |
| `stylesheet` | `(source: string) => string` | Validates CSS and returns it unchanged. Accepts a string or a tagged template literal, so authoring errors surface at module load. |
| `PageNumber` | `() => Element` | Resolves to the current page number inside a header or footer. |
| `TotalPages` | `() => Element` | Resolves to the final page count inside a header or footer. |

`RenderOptions` is `{ pageFormat?: 'A4' | 'Letter'; margin?: number; stylesheets?: readonly string[]; fonts?: readonly EmbeddedFont[]; header?: Element; footer?: Element; metadata?: PdfMetadata; tagged?: boolean; pdfa?: '3b'; attachments?: readonly Attachment[]; facturx?: FacturX }`; see [PDF/A and e-invoices](#pdfa-and-e-invoices) for the last three. An `EmbeddedFont` is `{ family: string; regular: Uint8Array; bold?: Uint8Array }`. Margins are points, and the default is 36. Headers and footers each accept one styled text line with optional page tokens. They repeat on every page and reserve their measured height before body pagination.

```tsx
import { PageNumber, TotalPages } from '@pettersen3008/flashpdf';

const pdf = await render(<Invoice />, {
  header: <header style={{ fontSize: '9pt' }}>Acme invoice</header>,
  footer: <footer style={{ fontSize: '9pt', textAlign: 'center' }}>Page <PageNumber /> of <TotalPages /></footer>,
  metadata: { title: 'Invoice 42', author: 'Acme', language: 'en-US' },
  tagged: true,
});
```

`PdfMetadata` accepts `title`, `author`, `subject`, `keywords`, and `language` strings. Tagged mode requires `language`. It writes a structure tree with paragraph lines and figures, plus image descriptions and page furniture marked as artifacts. This is basic tagging, not PDF/UA conformance: headings, tables, and link annotations do not yet have full semantic structure. Use an accessibility checker before distributing documents that must meet an accessibility standard.

## Embedded fonts

The host provides full TTF bytes for each family. `fontFamily` accepts a fallback list and selects the first registered family; `fontWeight: 'bold'` selects its optional bold file. Helvetica and Helvetica-Bold remain the fallback when `fontFamily` is absent.

```ts
const regular = new Uint8Array(await (await fetch('/fonts/invoice-regular.ttf')).arrayBuffer());
const bold = new Uint8Array(await (await fetch('/fonts/invoice-bold.ttf')).arrayBuffer());
const pdf = await render(<main style={{ fontFamily: 'Invoice' }}><h1>Invoice</h1></main>, {
  fonts: [{ family: 'Invoice', regular, bold }],
});
```

Each embedded font is subset to the glyphs the document shows (plus the components of composite glyphs) and written as a Type0/CIDFontType2 font with `Identity-H` encoding and a `ToUnicode` map, so copy-paste and text extraction return the original characters. Any character the TTF has a glyph for renders: Latin extended, Greek, Cyrillic, CJK, symbols, currency, and supplementary-plane characters through cmap formats 4 and 12. A character without a glyph rejects naming the character, its code point, and the element.

Lines break at ASCII whitespace and between CJK characters (ideographs, kana, Hangul, full-width forms), so unspaced CJK paragraphs wrap; U+00A0 never breaks. There is no shaping, kerning, ligature substitution, or bidi: each character maps to one glyph, combining marks render as spacing glyphs, and Arabic, Indic, or right-to-left text will not look right. Helvetica and Helvetica-Bold have no embedded program and stay limited to WinAnsi, so non-Latin text needs an embedded font. Regular and bold faces only; italic and variable-face selection, absent families, and a bold request without a bold file reject clearly.

## PDF/A and e-invoices

`pdfa: '3b'` writes PDF/A-3b: an XMP packet that repeats `metadata`, an sRGB output intent, and a document ID hashed from the file's content, so the same input still gives the same bytes. FlashPDF writes no creation or modification date. PDF/A needs every font embedded, so register one with `fonts` and set `fontFamily` on the root element; text that would fall back to Helvetica rejects. CMYK JPEGs also reject, because the output intent is RGB.

`attachments` embeds files as associated files, listed in the catalog's `/AF` array and in the viewer's attachments panel. Each entry is `{ name: string; data: Uint8Array; mimeType: string; description?: string; relationship: 'Source' | 'Data' | 'Alternative' | 'Supplement' | 'Unspecified' }`. Names must be unique and free of slashes and control characters, and all attachments together are capped at 64 MiB per render. Attachments also work without `pdfa`.

`facturx: { xml, profile }` makes a Factur-X or ZUGFeRD 2.x hybrid invoice and implies `pdfa: '3b'`. FlashPDF embeds the CII XML as `factur-x.xml` (`xrechnung.xml` for the `XRECHNUNG` profile) with the relationship the profile requires (`Data` for `MINIMUM` and `BASIC WL`, `Alternative` for the rest), and writes the Factur-X XMP extension schema with `fx:Version` 1.0. `profile` is `'MINIMUM'`, `'BASIC WL'`, `'BASIC'`, `'EN 16931'`, `'EXTENDED'`, or `'XRECHNUNG'`.

```tsx
const xml = new Uint8Array(await readFile('invoice-42.xml'));
const pdf = await render(<Invoice />, {
  fonts: [{ family: 'Invoice', regular, bold }],
  metadata: { title: 'Invoice 42', author: 'Acme', language: 'de-DE' },
  facturx: { xml, profile: 'EN 16931' },
});
```

FlashPDF only checks that `xml` is UTF-8 text that starts with `<` and ends with `>`. It does not validate the XML against the CII schema or the profile's rules, and it does not compare the XML with the rendered invoice. Validate the XML with your e-invoicing toolchain, for example the Mustang or KoSIT validator, before you send it.

## Compatibility

| Target | `render` |
| --- | --- |
| Browser via Vite, webpack, or Rollup | Yes, including host-provided `Uint8Array` TTFs, the bundler emits the WASM as an asset |
| Node 20.19+ | Yes, including host-provided `Uint8Array` TTFs |
| Bun 1.x | Yes, including host-provided `Uint8Array` TTFs |
| AWS Lambda Node.js 20.19+ | Yes, include the package's `wasm/` directory in the deployment artifact |
| Cloudflare Workers | Yes, Wrangler selects the `workerd` export and uploads the imported WASM module |
| Vercel Edge | Untested. The `edge-light` condition selects the same static WASM import as Cloudflare Workers, but no test runs it |

Every target runs the same decoder. `tests/e2e.mjs` installs a packed tarball, renders one invoice with an embedded font, a PNG, and a link, and requires the bytes to match Node's from the AWS Lambda handler, Bun, Chromium (built with Vite), and Wrangler's local Workers runtime. Vercel Edge has no test, so treat it as expected to work rather than proven. Content streams and embedded fonts are Flate-compressed. Lambda bundlers must copy `node_modules/@pettersen3008/flashpdf/wasm/` beside the package output. Wrangler handles its static WASM import automatically.

Bundler setup (Vite, webpack, Next.js), the browser CSP, fixes for common rejections, recipes, and a react-pdf migration map live in the [guide](./docs/guide.md).

## Assets

FlashPDF embeds host-provided regular and bold TTFs, and keeps Helvetica and Helvetica-Bold as the default. Images are PNG or JPEG bytes passed through `img`; FlashPDF never fetches a URL or reads a path.

Supported CSS, the intentional v1 limits, and the reason behind each rejection live in [CSS.md](./CSS.md).

## Examples

- [`packages/flashpdf/examples/native-invoice.tsx`](./packages/flashpdf/examples/native-invoice.tsx) is a complete invoice template using only inline styles. Pass `{ fonts: [{ family, regular, bold }] }` as its second argument to use an embedded invoice font. The clean-install test compiles and renders it on every run.
- [`packages/flashpdf/examples/css-invoice.tsx`](./packages/flashpdf/examples/css-invoice.tsx) is the same document driven by a CSS module.

## Working on FlashPDF

Requirements: Node 22.12+, pnpm 12.4.2 (pinned in `packageManager`), Rust 1.94 (pinned in `rust-toolchain.toml`), and `wasm-pack`. Bun 1.x, qpdf, and mupdf-tools (`mutool`) are optional locally, each enabling a check in `tests/e2e.mjs`, and CI requires all three. The package itself runs on Node 20.19+.

pnpm 10 cannot switch itself to pnpm 12, and Corepack 0.34 cannot launch it, so install the pinned version directly.

```bash
npm install -g pnpm@12.4.2
pnpm install --frozen-lockfile
pnpm exec playwright install chromium
sh verify.sh
```

That formats, lints, and tests the Rust workspace, formats and lints the TypeScript, rebuilds and size-gates the WASM, typechecks, runs the CSS build-step examples, runs the Vitest unit and integration projects, and finally runs the packed-package E2E test in Node, AWS Lambda packaging, Bun, Wrangler, and Chromium.

Run the isolated renderer comparison with `pnpm --filter @pettersen3008/flashpdf bench`. Every library renders the same fixture: A4 page, 36pt margins, 10pt body text, a 24pt heading, and a 70pt right-aligned amount column, so the comparison is fair even though each library uses its native authoring API. It reports median and p95 cold start and warm render, peak RSS, package size (`installBytes` adds every transitive runtime dependency, deduped by real path), and output PDF size for invoice and multi-page report fixtures. Set `FLASHPDF_BENCH_COLD_RUNS` or `FLASHPDF_BENCH_RUNS` to change the sample counts.

To publish the verified package, create a GitHub Release with a `v*` tag from a commit on `main`. The release workflow checks the tag against the package version and against a `## <version>` heading in `packages/flashpdf/CHANGELOG.md`, reruns `verify.sh`, attests build provenance for the packed tarball, and publishes that tarball to GitHub Packages with `GITHUB_TOKEN`.

`tests/golden/css-invoice.txt` pins the PDF content stream of the CSS invoice: every draw position, colour, font, and page break. Regenerate it with `UPDATE_GOLDEN=1` and review the diff.

See [AGENTS.md](./AGENTS.md) for the repository map and contribution rules.

## License

MIT. See [LICENSE](./LICENSE).
