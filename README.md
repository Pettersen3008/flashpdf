# FlashPDF

Render native JSX and a static CSS subset to a PDF. The layout engine is Rust compiled to WebAssembly, so the same code produces the same bytes in a browser, in Node, and in Bun.

Build it from a clone with the commands in [Working on FlashPDF](#working-on-flashpdf).

## Quickstart

```tsx
import { render } from '@pettersen3008/flashpdf';

function Invoice({ total }: { total: string }) {
  return (
    <main style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <h1>Invoice</h1>
      <div style={{ display: 'flex', flexDirection: 'row' }}>
        <span style={{ flex: 1 }}>Consulting</span>
        <span style={{ width: '90pt', textAlign: 'right' }}>{total}</span>
      </div>
    </main>
  );
}

const pdf = await render(<Invoice total="EUR 1200.00" />, { pageFormat: 'A4', margin: 36 });
```

This is standard React JSX. FlashPDF resolves pure function components and fragments without mounting a DOM. Effects, browser layout, and hook state do not belong in a static PDF template.

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

FlashPDF rejects any declaration it cannot honour, naming the property, the selector, and the source position. It never silently drops one.

## API

| Export | Signature | Notes |
| --- | --- | --- |
| `render` | `(element: ReactElement \| Iterable<ReactElement>, options?: RenderOptions) => Promise<Uint8Array>` | The root component must resolve to one or more host elements. Loads the WASM module once per process. |
| `stylesheet` | `(source: string) => string` | Validates CSS and returns it unchanged, for tagging literals at author time. |

`RenderOptions` is `{ pageFormat?: 'A4' | 'Letter'; margin?: number; stylesheets?: readonly string[]; fonts?: readonly EmbeddedFont[] }`. An `EmbeddedFont` is `{ family: string; regular: Uint8Array; bold?: Uint8Array }`. Margins are points, and the default is 36.

## Embedded fonts

The host provides full TTF bytes for each family. `fontFamily` selects an exact registered family; `fontWeight: 'bold'` selects its optional bold file. Helvetica and Helvetica-Bold remain the fallback when `fontFamily` is absent.

```ts
const regular = new Uint8Array(await (await fetch('/fonts/invoice-regular.ttf')).arrayBuffer());
const bold = new Uint8Array(await (await fetch('/fonts/invoice-bold.ttf')).arrayBuffer());
const pdf = await render(<main style={{ fontFamily: 'Invoice' }}><h1>Invoice</h1></main>, {
  fonts: [{ family: 'Invoice', regular, bold }],
});
```

v1 embeds whole TrueType files and keeps the existing WinAnsi text subset. It supports regular and bold faces only, not font-family fallback lists, italic/variable-face selection, OpenType features, or Unicode shaping. Unsupported characters, absent families, and a bold request without a bold file reject clearly.

## Compatibility

| Target | `render` |
| --- | --- | --- |
| Browser via Vite, webpack, or Rollup | Yes, including host-provided `Uint8Array` TTFs, the bundler emits the WASM as an asset |
| Node 20+ | Yes, including host-provided `Uint8Array` TTFs |
| Bun 1.x | Yes, including host-provided `Uint8Array` TTFs |

Every target runs the same decoder, so a document that renders in one produces identical bytes in the others. `tests/e2e.mjs` proves this against a packed tarball with Node, Bun, Vite, and Chromium.

## Assets

v1 embeds host-provided regular and bold TTFs, and keeps Helvetica and Helvetica-Bold as the default. There is no image input.

Supported CSS, the intentional v1 limits, and the reason behind each rejection live in [CSS.md](./CSS.md).

## Examples

- [`packages/flashpdf/examples/native-invoice.tsx`](./packages/flashpdf/examples/native-invoice.tsx) is a complete invoice template using only inline styles. Pass `{ fonts: [{ family, regular, bold }] }` as its second argument to use an embedded invoice font. The clean-install test compiles and renders it on every run.
- [`packages/flashpdf/examples/css-invoice.tsx`](./packages/flashpdf/examples/css-invoice.tsx) is the same document driven by a CSS module.

## Working on FlashPDF

Requirements: Node 22.12+, pnpm, Rust, and `wasm-pack`. Bun 1.x is optional for
the Bun-specific check.

```bash
pnpm install --frozen-lockfile
pnpm exec playwright install chromium
sh verify.sh
```

That formats and lints the Rust workspace, runs the Vitest unit and integration projects, rebuilds and size-gates the WASM, and finally runs the packed-package E2E test in Node, Bun, and Chromium.

To publish the verified public package:

```bash
npm publish ./packages/flashpdf
```

`tests/golden/css-invoice.txt` pins the PDF content stream of the CSS invoice: every draw position, colour, font, and page break. Regenerate it with `UPDATE_GOLDEN=1` and review the diff.

See [AGENTS.md](./AGENTS.md) for the repository map and contribution rules.

## License

MIT. See [LICENSE](./LICENSE).
