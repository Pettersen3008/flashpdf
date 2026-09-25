# FlashPDF

Render invoices and reports from native JSX and a static CSS subset. A Rust layout engine compiled to WebAssembly produces the same PDF bytes in browsers, Node, and Bun.

Build it from a clone with the commands in the repository [README](https://github.com/pettersen3008/flashpdf#working-on-flashpdf).

## Quickstart

```tsx
/** @jsxImportSource @pettersen3008/flashpdf */
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

## Install

Configure npm for the GitHub Packages scope in your project `.npmrc` and set `NODE_AUTH_TOKEN` to a classic GitHub personal access token with `read:packages`:

```ini
@pettersen3008:registry=https://npm.pkg.github.com
//npm.pkg.github.com/:_authToken=${NODE_AUTH_TOKEN}
```

Then install:

```sh
npm install @pettersen3008/flashpdf
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

Runnable Tailwind, StyleX, and styled-components build-step examples live in [`examples/css-integrations`](./examples/css-integrations). They compile or extract static CSS, then pass that string through `render({ stylesheets })`.

FlashPDF rejects any declaration it cannot honour, naming the property, the selector, and the source position. It never silently drops one.

## API

| Export | Signature | Notes |
| --- | --- | --- |
| `render` | `(element: Element \| Iterable<Element>, options?: RenderOptions) => Promise<Uint8Array>` | The root component must resolve to one or more host elements. Loads the WASM module once per process. |
| `stylesheet` | `(source: string) => string` | Validates CSS and returns it unchanged, for tagging literals at author time. |
| `PageNumber` | `() => Element` | Resolves to the current page number inside a header or footer. |
| `TotalPages` | `() => Element` | Resolves to the final page count inside a header or footer. |

`RenderOptions` accepts page format, margin, stylesheets, embedded fonts, a one-line repeating `header` and `footer`, `metadata`, and `tagged`. Margins are points, and the default is 36. `PdfMetadata` accepts title, author, subject, keywords, and language. Tagged mode requires a language and writes paragraph-line and figure tags with image `alt` text; it is not PDF/UA conformant.

```tsx
import { PageNumber, TotalPages } from '@pettersen3008/flashpdf';

const pdf = await render(<Invoice />, {
  header: <header style={{ fontSize: '9pt' }}>Acme invoice</header>,
  footer: <footer style={{ fontSize: '9pt', textAlign: 'center' }}>Page <PageNumber /> of <TotalPages /></footer>,
  metadata: { title: 'Invoice 42', language: 'en-US' },
  tagged: true,
});
```

## Embedded fonts

The host provides TTF bytes for each family. `fontFamily` selects the first registered family in a list; `fontWeight: 'bold'` selects its optional bold file. Helvetica and Helvetica-Bold remain the fallback when `fontFamily` is absent.

```ts
const regular = new Uint8Array(await (await fetch('/fonts/invoice-regular.ttf')).arrayBuffer());
const bold = new Uint8Array(await (await fetch('/fonts/invoice-bold.ttf')).arrayBuffer());
const pdf = await render(<main style={{ fontFamily: 'Invoice' }}><h1>Invoice</h1></main>, {
  fonts: [{ family: 'Invoice', regular, bold }],
});
```

Embedded TrueType files are subset to used glyphs and support Unicode through cmap formats 4 and 12. Regular and bold faces are supported; italic/variable-face selection and Unicode shaping are not.

## Compatibility

| Target | `render` |
| --- | --- | --- |
| Browser via Vite, webpack, or Rollup | Yes, including host-provided `Uint8Array` TTFs, the bundler emits the WASM as an asset |
| Node 20+ | Yes, including host-provided `Uint8Array` TTFs |
| Bun 1.x | Yes, including host-provided `Uint8Array` TTFs |
| AWS Lambda Node.js 20+ | Yes, include the package's `wasm/` directory in the deployment artifact |
| Cloudflare Workers | Yes, Wrangler selects the `workerd` export and uploads the imported WASM module |

Every target runs the same decoder, so a document that renders in one produces identical bytes in the others. `tests/e2e.mjs` proves this against a packed tarball with Node, AWS Lambda handler packaging, Bun, Vite, Chromium, and Wrangler's local Workers runtime. Lambda bundlers must copy `node_modules/@pettersen3008/flashpdf/wasm/` beside the package output. Wrangler handles its static WASM import automatically.

## Assets

Host-provided TTF, PNG, and JPEG bytes work in every supported runtime. FlashPDF never fetches assets itself. Images use `<img src={bytes} alt="description" />`.

Supported CSS, the intentional v1 limits, and the reason behind each rejection live in [CSS.md](https://github.com/pettersen3008/flashpdf/blob/main/CSS.md).

## Examples

- [`examples/native-invoice.tsx`](https://github.com/pettersen3008/flashpdf/blob/main/packages/flashpdf/examples/native-invoice.tsx) is a complete invoice template using only inline styles. Pass `{ fonts: [{ family, regular, bold }] }` as its second argument to use an embedded invoice font. The clean-install test compiles and renders it on every run.
- [`examples/css-invoice.tsx`](https://github.com/pettersen3008/flashpdf/blob/main/packages/flashpdf/examples/css-invoice.tsx) is the same document driven by a CSS module.

## License

MIT. See [LICENSE](./LICENSE).
