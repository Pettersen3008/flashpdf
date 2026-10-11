# FlashPDF CSS compatibility

FlashPDF renders a static CSS subset. It does not embed a browser engine, fetch assets, execute JavaScript in CSS, or support media/container queries, Grid, positioned layout, transforms, filters, animations, or SVG layout.

The public authoring API is native JSX plus `render`. `PageNumber` and `TotalPages` work only in a repeating `RenderOptions.header` or `RenderOptions.footer`, and the characters they stand for, U+001E and U+001F, reject in your own text. There are no `Document`, `Text`, `Stack`, `Row`, preview, or download helpers; tables use the HTML `table` tags.

## Authoring

FlashPDF supports a small static JSX vocabulary, not browser HTML compatibility. Use `main`, `div`, `section`, `article`, `header`, `footer`, `p`, headings, `span`, `b`, `strong`, `a`, `br`, `hr`, and `img`. A paragraph is a sequence of inline runs: adjacent text, `span`, `b`, and `strong` nodes flow together and wrap as one, each run keeping its own font, weight, size, and colour, so `<p>Hello <b>World</b></p>` is one line. A mixed line takes the tallest run's height. `br` forces a line break; `br` and `hr` take no children, and `br` takes no box or layout styles. Whitespace collapses to one space across runs and is trimmed at line ends. Whitespace-only text between block-level children or flex items, such as Prettier's `{" "}`, renders nothing, as in CSS. A `span` with margin, padding, border, background, `width`, `flex`, or its own `text-align` remains a standalone layout node. A word wider than its column breaks at the last glyph that fits. [Page breaks](#page-breaks) describes where content splits across pages.

Tables use `table`, `thead`, `tbody`, `tfoot`, `tr`, `th`, and `td`. Column widths come from the first row's cells: `width` in `pt`, `px`, `rem`, or `%`, or `flex`, exactly as flex-row children; cells without either share the remainder equally. Every row must have that many cells, and `colSpan`/`rowSpan` reject, as does a `td` or `th` outside a table row. A cell holds text, inline runs, or nested blocks; a row is as tall as its tallest cell and every cell stretches to it, so cell backgrounds and borders tile the row. A table splits between rows: `thead` rows repaint at the top of every page it continues on, `tfoot` rows repaint after the last body row of every page, and a row that does not fit moves whole to the next page under the repeated header. A page gets the header and footer only with at least one body row, so neither is ever left alone. `th` is bold and left-aligned by default. A `td`/`th` accepts padding, border, background, and text properties; a `tr` accepts a background that paints under its cells; a `table` accepts `width`, vertical margins, and `break-inside: avoid`. Text wraps inside a narrow cell rather than clipping.

```tsx
<table style={{ width: "100%" }}>
  <thead>
    <tr>
      <th>Item</th>
      <th style={{ width: "80pt", textAlign: "right" }}>Amount</th>
    </tr>
  </thead>
  <tbody>
    {items.map((item) => (
      <tr key={item.id}>
        <td>{item.name}</td>
        <td style={{ textAlign: "right" }}>{item.total}</td>
      </tr>
    ))}
  </tbody>
</table>
```

Images use `img` with `src` as the PNG or JPEG file bytes in a `Uint8Array`, passed by the host exactly like fonts: FlashPDF fetches nothing, and a string path or `data:` URL rejects with a hint. In tagged mode, nonempty `alt` becomes the figure description and empty `alt` marks a decorative image. An image is an atomic block box, left-aligned in its container. `width` and `height` take `pt`, `px`, or (width only) `%` of the container width; given one, the other follows the image's aspect ratio; given neither, the pixel size at 96 px per inch applies, capped to the container width. Images work as block children, in table cells, in flex rows, and in decorated boxes, and a paragraph splits around them at page ends, but an image cannot sit inside a paragraph's text. The same `Uint8Array` used on every row embeds once.

Links use `a` with an absolute `http:`, `https:`, or `mailto:` `href`. Inside a paragraph an `a` is an inline run like `b`; around an `img` or a block it makes that content clickable. Every wrapped line of a link gets its own rectangle. `a` defaults to `color: #0000EE` and `text-decoration: underline`, and a rule or inline style overrides both.

```tsx
<a href="https://example.com/invoices/42">
  <img src={logoBytes} alt="Acme Supply Co." style={{ width: 120 }} />
</a>
<p>
  Questions? <a href="mailto:billing@example.com">Email billing</a>
</p>
```

Compile CSS in the consuming app's build step, then pass the resulting string to `render`. CSS Modules and minification belong to that build step. `stylesheet` validates the final CSS against FlashPDF's supported subset.

```ts
import { render } from '@pettersen3008/flashpdf';

const css = '.invoice { padding: 18pt; background: #fff }';
const pdf = await render(<main className="invoice">Invoice</main>, {
  stylesheets: [css],
});
```

The CSS may come from CSS Modules or any tool that emits static CSS. Runnable build-step examples cover [Tailwind](./packages/flashpdf/examples/css-integrations/tailwind.mjs), [StyleX](./packages/flashpdf/examples/css-integrations/stylex.mjs), and [styled-components](./packages/flashpdf/examples/css-integrations/styled-components.mjs). Run all three with `pnpm --filter @pettersen3008/flashpdf examples:css`.

Tailwind's example imports `tailwindcss/theme.css` with `theme(inline)`, so colour utilities carry their `oklch()` values instead of `var()` references to a `:root` rule FlashPDF skips, and `tailwindcss/utilities.css` with `source(none)`, so it builds only the listed utilities, unlayered. `stylesheet()` accepts that build. Minify it: Tailwind then folds most `calc()` spacing into plain lengths, while unminified spacing utilities and the `line-height` of `text-sm` and its siblings keep `calc()` and reject where a rule applies. Utilities with an escaped name, such as `w-1/2`, never match, and `border` needs `border-style`, which the subset lacks. StyleX passes Babel metadata through `processStylexRules`. styled-components' server output also contains hydration metadata with unsupported attribute selectors and `content`; its example selects the generated component rule before calling `render`. FlashPDF adds no runtime adapter for any of them.

Unsupported declarations fail at `render` only for rules that match an element, naming the property, selector, and source position. `stylesheet(source)` validates the properties and values of every rule with a supported selector up front. Custom property definitions and values that use `var()`, `calc()`, `min()`, `max()`, `clamp()`, or `color-mix()` are checked by `render` where a rule applies, since only the cascade resolves them and Tailwind puts them in most utilities. Property names are case-insensitive, and a camelCase name such as `fontSize` rejects with its kebab-case spelling. At-rules, unsupported selectors (`*`, `:root`, pseudo-classes, attribute selectors, escaped names), and type selectors for tags FlashPDF never renders (`html`, `body`, `button`) are skipped:

```
unsupported CSS property: letter-spacing (text shaping is not implemented) in selector ".total" at 4:3
```

Every runtime error names the element and its ancestors, for example `on <span.total> in <div.row> > <main.invoice>`. FlashPDF never drops a declaration that applies to an element it renders.

## Page breaks

FlashPDF follows [CSS Fragmentation Level 3](https://www.w3.org/TR/css-break-3/) for the properties below. A page can break between sibling blocks, between table rows, and between the lines of a paragraph. It never breaks between a box's top border and padding and its first child, or between its last child and its bottom padding and border, so no page ends with only a box's top edge or starts with only its bottom edge.

- A box that does not fit the space left breaks at the page end, like CSS `box-decoration-break: slice`. The first page draws its top border and padding, the last page its bottom ones, and every page its side borders, side padding, and background, with the content continuing below the page header. Nested boxes break the same way.
- `orphans` and `widows`, 2 by default and inherited, are the fewest lines of a paragraph a break leaves at the bottom of a page and carries to the top of the next. A paragraph that cannot keep both moves whole. `orphans: 1; widows: 1` breaks wherever the page ends.
- `break-inside: avoid` keeps a box, paragraph, table, or `tbody` on one page: it moves to the next page, and rejects when taller than an empty page. Flex rows and table rows never break inside.
- `break-before` and `break-after` take `auto`, `page`, `avoid`, and `avoid-page`. `page` forces a break, also inside a box, which continues on the next page; a forced break with nothing above it on its page does nothing, so two in a row make one. `avoid` keeps an element with the content on the other side of that break: a heading with the first lines of the next paragraph, honouring `orphans`, a table row with the next row, or a paragraph's last `widows` lines with the block after it. A first child's `break-before` and a last child's `break-after` apply to their parent's edge too, as CSS propagates them.
- `h1` to `h6` default to `break-after: avoid`, as print style sheets do: the CSS 2.1 sample style sheet, Prince, and WeasyPrint. The HTML Standard's rendering rules and browsers' style sheets set no break properties, so a browser prints a heading alone at the bottom of a page; `break-after: auto` restores that.
- Margins next to an unforced break truncate to zero. After a forced break they remain.
- `avoid-page` means `avoid`, because a page is the only fragment. `left`, `right`, `recto`, and `verso` reject: pages have no sides.

When the rules leave no break point on a page, FlashPDF drops `orphans` and `widows` first and then the avoid rules, as CSS allows, instead of losing content. It decides with at most one page of content held back: content after the page's last allowed break point waits, unpainted, until the next allowed break point arrives. A run of avoided breaks taller than a page, or holding more than 16 MiB of layout, therefore breaks anyway.

## Supported subset

- Tags (case-insensitive, as in HTML), `.class`, `#id`, compound, descendant, and child selectors.
- Specificity as an (ids, classes, tags) tuple, then source order across the whole `stylesheets` array, so a later sheet wins a tie. Inline styles beat both, and `!important` declarations beat all non-important ones. A shorthand and its longhands cascade in that order too, so a later `border: none` clears an earlier `border-bottom`, and a later `background` replaces `background-color`. An inline value of `undefined` is skipped, as React does.
- Inherited text properties and `var(--name, fallback)`. The fallback may hold functions and a nested `var()`, such as `var(--c, rgb(0, 0, 0))` or `var(--a, var(--b))`.
- `inherit` and `initial` on every supported property. `inherit` takes the parent's value, and an `em` length the parent declared resolves again against this element's font size. `currentcolor` is the element's own `color`.
- `pt`, `px`, `em`, `rem`, unitless values where accepted, and percent widths in flex rows. `width` and image `height` take `pt`, `px`, `rem`, or (width only) `%`.
- Block flow, flex rows/columns, `gap` between flex row items and between column children, page size, and document margins.
- `line-height` as `normal`, a unitless factor, a percent, or a length. A factor inherits as a factor; a percent or `em` resolves against the element's own font size and inherits as a length. The extra height splits evenly above and below the text, as CSS half-leading does.
- `width` and `flex` on the direct children of a `flex-direction: row` box, which is where columns are measured. Both reject anywhere else rather than being dropped. The `flex` shorthand keeps only the grow factor; grow 0 needs a `width`. A column cannot keep a basis, so a grow above 0 with a length basis (`flex: 1 1 100px`), or with `auto` beside a `width`, rejects. `flex: 1` has a zero basis, so a `width` beside it is ignored, as in CSS.
- Margin, padding, solid uniform borders, solid backgrounds, text color, `font-family`, size, and alignment. `font-weight` takes `normal`, `bold`, or a number from 1 to 1000, where 600 and up is bold; anything else, `bolder` and `lighter` included, rejects.
- `text-decoration: none | underline | line-through`, alone or combined, on any text element, and `text-decoration-line` with the same values. The rule is 1/14 em thick, an underline sits 1/8 em below the baseline, and both take the run's colour. Descendants inherit it.
- `img`: `width` anywhere (`pt`, `px`, `rem`, `%`) and `height` (`pt`, `px`, `rem`), which only `img` accepts. PNG at 8 bits per channel: greyscale, RGB, and palette pass through undecoded with `/Predictor 15`; greyscale+alpha, RGBA, and a palette with `tRNS` split the alpha into an `/SMask`; a greyscale or RGB `tRNS` colour key becomes `/Mask`. JPEG baseline and progressive pass through as `/DCTDecode` in greyscale, RGB, or CMYK, with Adobe APP14 CMYK inverted through `/Decode`.
- `a`: `href` as an absolute `http:`, `https:`, or `mailto:` URL, emitted as one `/Link` annotation per line segment or image. An `http:` or `https:` URL must spell out its host, so `http:foo` rejects. `font-family` lists use the first registered `RenderOptions.fonts` family; `Helvetica`, `Arial`, and generic families fall back to Helvetica. Colours accept hex, `rgb()`, `hsl()`, `oklch()`, `oklab()`, `currentcolor`, and CSS named colours; OKLab colours outside sRGB clip per channel, and opacity rejects.
- `h1`-`h6` defaults of `2em`/`1.5em`/`1.17em`/`1em`/`0.83em`/`0.67em` and bold, which a matching rule or inline style overrides but an inherited `font-size` does not.
- `break-before` and `break-after` as `auto`, `page`, `avoid`, or `avoid-page`; `break-inside` as `auto`, `avoid`, or `avoid-page`; `orphans` and `widows` as whole numbers from 1. See [Page breaks](#page-breaks).
- Tables: `width` and `flex` on the first row's `th`/`td` size the columns; `padding`, `border`, `background`, and text properties on cells; `background` on `tr`; `width`, `margin-top`, `margin-bottom`, and `break-inside: avoid` on `table`; `break-inside: avoid` on `tbody` keeps its rows together; `break-before` and `break-after` on `tr` and `tbody`, where `avoid` keeps rows together and `page` rejects.

Block children stream independently, including those of boxes, so long `<main>` documents paginate without retaining the whole layout; the renderer holds back at most one page of placed content while a break is undecided. A table's body rows stream unless the table sits in a flex row or a `break-inside: avoid` group, and a streaming table repeats its header and footer rows on every page. Flex rows and `break-inside: avoid` groups remain atomic. A single open container, a table's header and footer rows, or one table row may buffer at most 16 MiB of layout commands, and a document may paint at most 16 MiB of content.

## Intentional v1 limits

- Helvetica and Helvetica-Bold are the fallback. Registered host-provided TTF families support regular and optional bold files. Italic/other faces, OpenType features, shaping, letter-spacing, and `text-decoration: overline` reject.
- Text in an embedded TTF renders any character the font has a glyph for, including CJK and supplementary-plane characters; a character without a glyph rejects, naming it and its code point. There is no shaping, kerning, ligature substitution, or bidi: each character maps to one glyph, combining marks render as spacing glyphs, and right-to-left text comes out in logical order. Helvetica and Helvetica-Bold stay WinAnsi standard fonts, so non-Latin text needs an embedded font. Lines break at ASCII whitespace and between CJK characters; U+00A0 does not break.
- Images: 16-bit, 1/2/4-bit, and interlaced PNGs, 12-bit, lossless, hierarchical, and arithmetic-coded JPEGs, and every other format (GIF, WebP, SVG) reject naming the feature. A PNG with transparency must decode to at most 64 MiB, and all images in one render are capped at 64 MiB of input. `object-fit` rejects: an image always scales to its box. An image wider than its container or taller than an empty page rejects with the element path. Image bytes are not CRC-checked; a corrupt file produces a corrupt picture, not an error.
- Links: internal `#anchor` links are not supported yet, and `javascript:`, relative, and non-ASCII-after-encoding URLs reject. Link rectangles have no border. Footer text takes no links or decoration.
- No border radius, gradients, `height` outside `img`, `min-width`, or `max-width`. `border-top`, `border-right`, `border-bottom`, and `border-left` accept the same solid syntax as `border`; `<hr>` renders a one-point black bottom rule, which its own border declarations override; it takes `break-before` and `break-after`, and `width` only as a flex row child.
- Tables have no `colSpan`/`rowSpan`, `colgroup`/`col`, or `caption`. `vertical-align` rejects: cells align to the top. `border-collapse` and `border-spacing` reject: cells tile the row with separate borders, so adjacent full borders double up; draw one edge per cell (`border-bottom`, and `border-left` on all but the first column) for single rules. `table` takes no padding, border, background, or horizontal margins; style its cells or a wrapping `div`. A table inside a flex row or a `break-inside: avoid` group is atomic like any other content there.
- `box-decoration-break` rejects: a split box always draws its top border and padding on the first page and its bottom ones on the last, as `slice` does. Rejected at-rules and unmatched rules are skipped silently; `@layer` block contents are skipped too, so import `tailwindcss/utilities` rather than the full preflight.
