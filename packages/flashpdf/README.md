# FlashPDF

Render invoices and reports from native JSX and a static CSS subset. A Rust layout engine compiled to WebAssembly produces the same PDF bytes in browsers, Node, and Bun.

Build it from a clone with the commands in the repository [README](https://github.com/pettersen3008/flashpdf#working-on-flashpdf).

## Quickstart

```tsx
/** @jsxImportSource @pettersen3008/flashpdf */
import { render } from "@pettersen3008/flashpdf";

function Invoice({ total }: { total: string }) {
  return (
    <main style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <h1>Invoice</h1>
      <div style={{ display: "flex", flexDirection: "row" }}>
        <span style={{ flex: 1 }}>Consulting</span>
        <span style={{ width: "90pt", textAlign: "right" }}>{total}</span>
      </div>
    </main>
  );
}

const pdf = await render(<Invoice total="EUR 1200.00" />, { pageFormat: "A4", margin: 36 });
```

## Tables

A `<table>` splits between rows and repeats its `<thead>` on every page. Each column takes its width from its first cell without `colSpan`, cells stretch to the row height, and `th` is bold by default. `colSpan` and `rowSpan` span cells, `vertical-align: top | middle | bottom` places a cell's content, and `border-collapse: collapse` draws one line between neighbouring cells.

```tsx
<table>
  <thead>
    <tr>
      <th>Description</th>
      <th style={{ width: "90pt", textAlign: "right" }}>Amount</th>
    </tr>
  </thead>
  <tbody>
    {items.map((item) => (
      <tr key={item.id}>
        <td style={{ borderBottom: "0.5pt solid #ddd" }}>{item.description}</td>
        <td style={{ borderBottom: "0.5pt solid #ddd", textAlign: "right" }}>{item.amount}</td>
      </tr>
    ))}
  </tbody>
</table>
```

Rows that a `rowSpan` joins move between pages together, and a `rowSpan` stays inside its `thead`, `tbody`, or `tfoot`. `vertical-align: baseline`, `border-spacing` other than 0, and `<colgroup>` reject with a reason; see [CSS.md](https://github.com/pettersen3008/flashpdf/blob/main/CSS.md).

## Images and links

`img` takes the PNG or JPEG bytes as a `Uint8Array`, like fonts, and `a` takes an absolute `http:`, `https:`, or `mailto:` URL. In tagged mode, a nonempty `alt` becomes the figure description; an empty `alt` marks the image as decoration.

```tsx
const logo = new Uint8Array(await readFile('logo.png'));
<a href="https://example.com/invoices/42">
  <img src={logo} alt="Acme Supply Co." style={{ width: 120 }} />
</a>
<p>Pay online at <a href="https://example.com/pay">the portal</a>.</p>
```

PNG at 8 bits per channel (greyscale, RGB, palette, and alpha variants) and baseline or progressive JPEG (greyscale, RGB, CMYK) embed without re-encoding. Other depths, interlaced PNGs, and other formats reject naming the feature; see [CSS.md](https://github.com/pettersen3008/flashpdf/blob/main/CSS.md).

## Element trees as data

`render` also takes the plain `{ type, props }` objects that JSX compiles to, so a tree can come from JSON without a build step. `type` is a tag name, and `props` holds the same attributes as JSX, with `children` as one child or an array of elements, strings, and numbers. The tree goes through the same checks as JSX: an unknown tag or attribute rejects with its element path, and nesting stops at 64 levels. The `ElementData` type describes one element.

```ts
import { render, type ElementData } from "@pettersen3008/flashpdf";

const tree: ElementData = JSON.parse(
  '{"type":"main","props":{"children":[{"type":"h1","props":{"children":"Invoice"}}]}}',
);
const pdf = await render(tree);
```

## HTML

A string passed to `render`, or to `header`, `footer`, `firstPageHeader`, and `firstPageFooter`, is HTML. The Rust core parses it into the same elements as JSX, so the same tags, CSS, and checks apply. Each `<style>` joins the cascade after `stylesheets`, and an `<img src>` names an entry in the `images` option, which holds PNG, JPEG, or SVG bytes. FlashPDF loads nothing from the network or the file system.

```ts
const pdf = await render(
  `<!DOCTYPE html>
  <style>.total { font-weight: bold }</style>
  <main>
    <img src="logo" alt="Acme" style="width: 90pt">
    <table>
      <tr><td>Consulting<td>EUR 1200.00
      <tr class="total"><td>Total<td>EUR 1200.00
    </table>
  </main>`,
  { images: { logo }, footer: "<p>Acme Supply Co. &middot; Oslo</p>" },
);
```

The parser is strict. It accepts what HTML lets an author leave out: the end tags of `p`, `li`, `dt`, `dd`, table sections, rows, and cells, an implied `tbody`, the `html`, `head`, and `body` wrappers, comments, and `<meta charset="utf-8">`. Markup a browser would repair silently rejects with its line and column instead: a misnested or missing end tag, `<div/>`, a bare `&`, an unknown character reference, element, or attribute, and inline `<svg>` (pass the SVG in `images`). Attributes use their HTML names: `class`, `colspan`, `rowspan`, `start`, `reversed`, `value`, `dir`, `href`, `alt`, and `data-bookmark`, where an empty value leaves the heading out of the outline. `href` is used as written, so it must already be a normalised absolute `http`, `https`, or `mailto` URL, or `#id`. Named character references cover the common typographic set, such as `&nbsp;`, `&euro;`, and `&mdash;`; any other character takes a numeric reference.

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
const url = URL.createObjectURL(new Blob([pdf], { type: "application/pdf" }));
```

## Styling with CSS

Compile stylesheets with the consuming app's build tool and pass the resulting string to `render`:

```tsx
import css from "./invoice.css?inline";
import { render } from "@pettersen3008/flashpdf";

const pdf = await render(<main className="invoice">…</main>, { stylesheets: [css] });
```

CSS Modules, minification, and syntax transforms belong to the frontend build. Lightning CSS works well as a Vite, webpack, or Rollup plugin. FlashPDF only validates the subset it can render.

Runnable Tailwind, StyleX, and styled-components build-step examples live in [`examples/css-integrations`](./examples/css-integrations). They compile or extract static CSS, then pass that string through `render({ stylesheets })`.

FlashPDF rejects any declaration it cannot honour on an element it renders, naming the property, selector, source position, and element path. Unused rules, at-rules, and selectors that match nothing in a PDF (`:hover`, `::before`) are skipped. A selector it cannot evaluate, such as `[href]` or `:has()`, rejects.

## API

| Export       | Signature                                                                                 | Notes                                                                                                 |
| ------------ | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `render`     | `(element: Element \| Iterable<Element> \| string, options?: RenderOptions) => Promise<Uint8Array>` | The root component must resolve to one or more host elements; a string is HTML. Loads the engine once per process. |
| `stylesheet` | `(source: string) => string`                                                              | Returns CSS unchanged; the first `render` that uses it checks every rule, matched or not.                          |
| `PageNumber` | `() => Element`                                                                           | Resolves to the current page number inside a header or footer.                                        |
| `TotalPages` | `() => Element`                                                                           | Resolves to the final page count inside a header or footer.                                           |

`RenderOptions` accepts page format, margin, stylesheets, embedded fonts, repeating `header` and `footer`, `metadata`, `tagged`, and `pdfua`. Margins are points, and the default is 36. `PdfMetadata` accepts title, author, subject, keywords, and language. `tagged: true` requires a language and writes basic paragraph-line and figure tags.

`pdfua: true` enables PDF/UA-1 semantic tags for headings, paragraphs across pages, lists, tables, links, figures, and inline language changes. It requires a nonempty `metadata.title`, `metadata.language`, embedded fonts selected with `fontFamily`, and explicit image `alt`. An empty `alt` marks a decorative image. Headings start at `h1` and cannot skip levels. Use `lang="nb"` for a language change and `scope="row"`, `"col"`, or `"both"` on a `th`, defaulting to `"col"`. Headers, footers, page numbers, and repeated table headers and footers are artifacts. Combine it with `pdfa: "2u"` or `"3b"`; 3b also supports attachments. Authors must check the document's reading order and the meaning of its headings, table headers, and alt text.

```tsx
import { PageNumber, TotalPages } from "@pettersen3008/flashpdf";

const pdf = await render(<Invoice />, {
  header: <header style={{ fontSize: "9pt" }}>Acme invoice</header>,
  footer: (
    <footer style={{ fontSize: "9pt", textAlign: "center" }}>
      Page <PageNumber /> of <TotalPages />
    </footer>
  ),
  metadata: { title: "Invoice 42", language: "en-US" },
  tagged: true,
});
```

## Embedded fonts

The host provides TTF bytes for each family. `fontFamily` selects the first entry in a list that is a registered family or a built-in name; `fontWeight: 'bold'` selects its optional bold file. Helvetica, Times (`serif`), and Courier (`monospace`) are built in, and Helvetica is the default.

```ts
const regular = new Uint8Array(await (await fetch('/fonts/invoice-regular.ttf')).arrayBuffer());
const bold = new Uint8Array(await (await fetch('/fonts/invoice-bold.ttf')).arrayBuffer());
const pdf = await render(<main style={{ fontFamily: 'Invoice' }}><h1>Invoice</h1></main>, {
  fonts: [{ family: 'Invoice', regular, bold }],
});
```

Embedded TrueType files are subset to used glyphs and support Unicode through cmap formats 4 and 12. Regular, bold, italic, and bold italic faces are supported, and a missing italic face is slanted from the upright one; variable-face selection and Unicode shaping are not.

## Compatibility

| Target                               | `render`                                                                                                            |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------- |
| Browser via Vite, webpack, or Rollup | Yes, including host-provided `Uint8Array` TTFs, the bundler emits the WASM as an asset                              |
| Node 20.19+                          | Yes, including host-provided `Uint8Array` TTFs                                                                      |
| Bun 1.x                              | Yes, including host-provided `Uint8Array` TTFs                                                                      |
| Deno 2                               | Yes, with `--allow-read`, plus `--allow-ffi` to load the native addon                                               |
| AWS Lambda Node.js 20.19+            | Yes, include the package's `wasm/` directory in the deployment artifact                                             |
| Cloudflare Workers                   | Yes, Wrangler selects the `workerd` export and uploads the imported WASM module                                     |
| Vercel Edge                          | Untested. The `edge-light` condition selects the same static WASM import as Cloudflare Workers, but no test runs it |

Node, Bun, and Deno load a prebuilt N-API addon from the `@pettersen3008/flashpdf-<platform>` optional dependency (macOS arm64 and x64, Linux x64 and arm64 with glibc) and fall back to the WASM where none loads. Both run the same Rust core and write the same bytes. Every target runs the same decoder. The repository's `tests/e2e.mjs` installs a packed tarball, renders one invoice with an embedded font, a PNG, and a link, and requires the bytes to match Node's from the AWS Lambda handler, Bun, Deno, Node on the WASM fallback, Chromium (built with Vite), and Wrangler's local Workers runtime. Vercel Edge has no test, so treat it as expected to work rather than proven. Content streams and embedded fonts are Flate-compressed. Lambda bundlers must copy `node_modules/@pettersen3008/flashpdf/wasm/` beside the package output. Wrangler handles its static WASM import automatically.

Bundler setup (Vite, webpack, Next.js), the browser CSP, fixes for common rejections, recipes, and a react-pdf migration map live in the [guide](https://github.com/pettersen3008/flashpdf/blob/main/docs/guide.md).

## Assets

Host-provided TTF, PNG, and JPEG bytes work in every supported runtime. FlashPDF never fetches assets itself. Images use `<img src={bytes} alt="description" />`.

Supported CSS, the intentional v1 limits, and the reason behind each rejection live in [CSS.md](https://github.com/pettersen3008/flashpdf/blob/main/CSS.md).

## Examples

- [`examples/native-invoice.tsx`](https://github.com/pettersen3008/flashpdf/blob/main/packages/flashpdf/examples/native-invoice.tsx) is a complete invoice template using only inline styles. Pass `{ fonts: [{ family, regular, bold }] }` as its second argument to use an embedded invoice font. The clean-install test compiles and renders it on every run.
- [`examples/css-invoice.tsx`](https://github.com/pettersen3008/flashpdf/blob/main/packages/flashpdf/examples/css-invoice.tsx) is the same document driven by a CSS module.

## License

MIT. See [LICENSE](./LICENSE).
