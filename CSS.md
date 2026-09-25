# FlashPDF CSS compatibility

FlashPDF renders a static CSS subset. It does not embed a browser engine, fetch assets, execute JavaScript in CSS, or support media/container queries, Grid, positioned layout, transforms, filters, animations, or SVG layout.

The public authoring API is native JSX plus `render`. `PageNumber` and `TotalPages` work only in the repeating `RenderOptions.footer`. There are no `Document`, `Text`, `Table`, `Stack`, `Row`, preview, or download helpers.

## Authoring

FlashPDF supports a small static JSX vocabulary, not browser HTML compatibility. Use `main`, `div`, `section`, `article`, `header`, `footer`, `p`, headings, `span`, and `hr`. Adjacent text and unstyled `span` nodes flow together; styled spans remain standalone layout nodes. Paragraphs and unpainted boxes split across pages line by line, and a word wider than its column breaks at the last glyph that fits. Flex rows, `break-inside: avoid` groups, and painted, padded, or bordered boxes are atomic: they move whole to the next page and reject only when taller than an empty page.

Compile CSS in the consuming app's build step, then pass the resulting string to `render`. CSS Modules and minification belong to that build step. `stylesheet` validates the final CSS against FlashPDF's supported subset.

```ts
import { render } from '@pettersen3008/flashpdf';

const css = '.invoice { padding: 18pt; background: #fff }';
const pdf = await render(<main className="invoice">Invoice</main>, {
  stylesheets: [css],
});
```

The CSS may come from CSS Modules or any tool that emits static CSS. Runnable build-step examples cover [Tailwind](./packages/flashpdf/examples/css-integrations/tailwind.mjs), [StyleX](./packages/flashpdf/examples/css-integrations/stylex.mjs), and [styled-components](./packages/flashpdf/examples/css-integrations/styled-components.mjs). Run all three with `pnpm --filter @pettersen3008/flashpdf examples:css`.

Tailwind's example builds only the utilities used by the PDF template. StyleX passes Babel metadata through `processStylexRules`. styled-components' server output also contains hydration metadata with unsupported attribute selectors and `content`; its example selects the generated component rule before calling `render`. FlashPDF adds no runtime adapter for any of them.

Unsupported declarations fail at `render` only for rules that match an element, naming the property, selector, and source position. `stylesheet(source)` validates every rule with a supported selector up front. At-rules and unsupported selectors (`*`, `:root`, pseudo-classes, attribute selectors, escaped names) are skipped:

```
unsupported CSS property: line-height (line height is fixed to the font's ascent, descent, and line gap) in selector ".total" at 4:3
```

Every runtime error names the element and its ancestors, for example `on <span.total> in <div.row> > <main.invoice>`. FlashPDF never drops a declaration that applies to an element it renders.

## Supported subset

- Tags, `.class`, `#id`, compound, descendant, and child selectors.
- Specificity as an (ids, classes, tags) tuple, then source order across the whole `stylesheets` array, so a later sheet wins a tie. Inline styles beat both, and `!important` declarations beat all non-important ones.
- Inherited text properties and `var(--name, fallback)`.
- `pt`, `px`, `em`, `rem`, unitless values where accepted, and percent widths in flex rows.
- Block flow, flex rows/columns, `gap`, page size, and document margins.
- `width` and `flex` on the direct children of a `flex-direction: row` box, which is where columns are measured. Both reject anywhere else rather than being dropped. The `flex` shorthand keeps only the grow factor; grow 0 needs a `width`.
- Margin, padding, solid uniform borders, solid backgrounds, text color, `font-family`, regular/bold font weight, size, and alignment. `font-family` lists use the first registered `RenderOptions.fonts` family; `Helvetica`, `Arial`, and generic families fall back to Helvetica. Colours accept hex, `rgb()`, `hsl()`, and CSS named colours; opacity rejects.
- `h1`-`h6` defaults of `2em`/`1.5em`/`1.17em`/`1em`/`0.83em`/`0.67em` and bold, which a matching rule or inline style overrides but an inherited `font-size` does not.
- `break-before`, `break-after`, and `break-inside: avoid`.

Normal block children stream independently, so long `<main>` documents paginate without retaining the whole layout. Flex rows and `break-inside: avoid` groups remain atomic. A single open container may buffer at most 16 MiB of layout commands, and a document may paint at most 16 MiB of content.

## Intentional v1 limits

- Helvetica and Helvetica-Bold are the fallback. Registered host-provided TTF families support regular and optional bold files. Italic/other faces, OpenType features, shaping, `line-height` (fixed to the font's ascent, descent, and line gap), letter-spacing, and text decoration reject.
- Text remains Latin/WinAnsi. Unsupported Unicode characters reject even when the embedded TTF contains them.
- No image component yet. Hosts must keep logos outside the generated PDF until a bounded image-byte protocol lands.
- No border radius, gradients, `height`, `min-width`, or `max-width`. `border-top`, `border-right`, `border-bottom`, and `border-left` accept the same solid syntax as `border`; `<hr>` renders a one-point bottom rule.
- `gap` on a flex row rejects. Use a fixed-width spacer column.
- A decorated box is atomic. Use document margins for page-wide padding and decorate individual cards/sections, not a multi-page root wrapper. Rejected at-rules and unmatched rules are skipped silently; `@layer` block contents are skipped too, so import `tailwindcss/utilities` rather than the full preflight.
