# FlashPDF

Render native JSX and a static CSS subset to a PDF. The layout engine is Rust compiled to WebAssembly, so the same code produces the same bytes in a browser, in Node, and in Bun.

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

The playground at https://pettersen3008.github.io/flashpdf/ renders a TSX template and a stylesheet to a PDF in the browser, with FlashPDF's validation errors shown in full. Run it locally with `pnpm playground`, share a template through the URL hash, and upload a `.ttf` to test embedded fonts. It deploys to GitHub Pages from `main` and lives in [`packages/playground`](./packages/playground).

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

## Install

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
| `PageNumber` | `() => Element` | Resolves to the current page number inside `RenderOptions.footer`. |
| `TotalPages` | `() => Element` | Resolves to the final page count inside `RenderOptions.footer`. |

`RenderOptions` is `{ pageFormat?: 'A4' | 'Letter'; margin?: number; stylesheets?: readonly string[]; fonts?: readonly EmbeddedFont[]; footer?: Element }`. An `EmbeddedFont` is `{ family: string; regular: Uint8Array; bold?: Uint8Array }`. Margins are points, and the default is 36. A footer accepts one styled text line, repeats at the bottom of every page, and reserves its measured height before body pagination.

```tsx
import { PageNumber, TotalPages } from '@pettersen3008/flashpdf';

const pdf = await render(<Invoice />, {
  footer: <footer style={{ fontSize: '9pt', textAlign: 'center' }}>Page <PageNumber /> of <TotalPages /></footer>,
});
```

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

## Compatibility

| Target | `render` |
| --- | --- |
| Browser via Vite, webpack, or Rollup | Yes, including host-provided `Uint8Array` TTFs, the bundler emits the WASM as an asset |
| Node 20.19+ | Yes, including host-provided `Uint8Array` TTFs |
| Bun 1.x | Yes, including host-provided `Uint8Array` TTFs |
| AWS Lambda Node.js 20.19+ | Yes, include the package's `wasm/` directory in the deployment artifact |
| Cloudflare Workers | Yes, Wrangler selects the `workerd` export and uploads the imported WASM module |
| Vercel Edge | Yes, the `edge-light` condition selects the same static WASM import |

Every target runs the same decoder, so a document that renders in one produces identical bytes in the others. Content streams and embedded fonts are Flate-compressed. `tests/e2e.mjs` proves this against a packed tarball with Node, AWS Lambda handler packaging, Bun, Vite, Chromium, and Wrangler's local Workers runtime. Lambda bundlers must copy `node_modules/@pettersen3008/flashpdf/wasm/` beside the package output. Wrangler handles its static WASM import automatically.

## Assets

v1 embeds host-provided regular and bold TTFs, and keeps Helvetica and Helvetica-Bold as the default. There is no image input.

Supported CSS, the intentional v1 limits, and the reason behind each rejection live in [CSS.md](./CSS.md).

## Examples

- [`packages/flashpdf/examples/native-invoice.tsx`](./packages/flashpdf/examples/native-invoice.tsx) is a complete invoice template using only inline styles. Pass `{ fonts: [{ family, regular, bold }] }` as its second argument to use an embedded invoice font. The clean-install test compiles and renders it on every run.
- [`packages/flashpdf/examples/css-invoice.tsx`](./packages/flashpdf/examples/css-invoice.tsx) is the same document driven by a CSS module.

## Working on FlashPDF

Requirements: Node 22.12+, pnpm, Rust 1.94 (pinned in `rust-toolchain.toml`), and `wasm-pack`. Bun 1.x is optional for the Bun-specific check, and qpdf is optional for the PDF structure check in `tests/e2e.mjs`. The package itself runs on Node 20.19+.

```bash
pnpm install --frozen-lockfile
pnpm exec playwright install chromium
sh verify.sh
```

That formats, lints, and tests the Rust workspace, formats and lints the TypeScript, rebuilds and size-gates the WASM, typechecks, runs the CSS build-step examples, runs the Vitest unit and integration projects, and finally runs the packed-package E2E test in Node, AWS Lambda packaging, Bun, Wrangler, and Chromium.

Run the isolated renderer comparison with `pnpm --filter @pettersen3008/flashpdf bench`. Every library renders the same fixture: A4 page, 36pt margins, 10pt body text, a 24pt heading, and a 70pt right-aligned amount column, so the comparison is fair even though each library uses its native authoring API. It reports median and p95 cold start and warm render, peak RSS, package size (`installBytes` adds every transitive runtime dependency, deduped by real path), and output PDF size for invoice and multi-page report fixtures. Set `FLASHPDF_BENCH_COLD_RUNS` or `FLASHPDF_BENCH_RUNS` to change the sample counts.

To publish the verified package, create a GitHub Release with a `v*` tag from a commit on `main`. The release workflow checks the tag against the package version, reruns `verify.sh`, and publishes to npm with provenance through trusted publishing.

`tests/golden/css-invoice.txt` pins the PDF content stream of the CSS invoice: every draw position, colour, font, and page break. Regenerate it with `UPDATE_GOLDEN=1` and review the diff.

See [AGENTS.md](./AGENTS.md) for the repository map and contribution rules.

## License

MIT. See [LICENSE](./LICENSE).
