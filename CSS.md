# FlashPDF CSS compatibility

FlashPDF renders a static CSS subset. It does not embed a browser engine, fetch assets, execute JavaScript in CSS, or support media/container queries, Grid, positioned layout, transforms, filters, animations, or SVG layout.

The public authoring API is native JSX plus `render`. `PageNumber` and `TotalPages` work only in a repeating `RenderOptions.header` or `RenderOptions.footer`, and the characters they stand for, U+001E and U+001F, reject in your own text. There are no `Document`, `Text`, `Stack`, `Row`, preview, or download helpers; tables use the HTML `table` tags.

## Authoring

FlashPDF supports a small static JSX vocabulary, not browser HTML compatibility. Use `main`, `div`, `section`, `article`, `header`, `footer`, `nav`, `aside`, `p`, headings, `pre`, `blockquote`, `address`, `figure`, `figcaption`, `ul`, `ol`, `li`, `dl`, `dt`, `dd`, `span`, `b`, `strong`, `em`, `i`, `cite`, `small`, `sub`, `sup`, `u`, `ins`, `s`, `del`, `mark`, `code`, `kbd`, `samp`, `a`, `br`, `hr`, and `img`. A paragraph is a sequence of inline runs: adjacent text and the inline elements (`span` through `samp`, and `a`) flow together and wrap as one, each run keeping its own font, weight, style, size, and colour, so `<p>Hello <b>World</b></p>` is one line. `em`, `i`, and `cite` are italic by default, as in browsers. A mixed line takes the tallest run's height. `br` forces a line break; `br` and `hr` take no children, and `br` takes no box or layout styles. Whitespace collapses to one space across runs and is trimmed at line ends. Whitespace-only text between block-level children or flex items, such as Prettier's `{" "}`, renders nothing, as in CSS. An inline element with margin, padding, border, background, `width`, `flex`, or its own `text-align` remains a standalone layout node; `mark` is the exception for its background, below. Paragraphs and unpainted boxes split across pages line by line, and a word wider than its column breaks at the last glyph that fits. Flex rows, `break-inside: avoid` groups, and painted, padded, or bordered boxes are atomic: they move whole to the next page and reject only when taller than an empty page.

Tables use `table`, `thead`, `tbody`, `tfoot`, `tr`, `th`, and `td`. Column widths come from the first row's cells: `width` in `pt`, `px`, or `%`, or `flex`, exactly as flex-row children; cells without either share the remainder equally. Every row must have that many cells, and `colSpan`/`rowSpan` reject, as does a `td` or `th` outside a table row. A cell holds text, inline runs, or nested blocks; a row is as tall as its tallest cell and every cell stretches to it, so cell backgrounds and borders tile the row. A table splits between rows: `thead` rows repaint at the top of every page it continues on, `tfoot` rows repaint after the last body row of every page, and a row that does not fit moves whole to the next page under the repeated header. `th` is bold and left-aligned by default. A `td`/`th` accepts padding, border, background, and text properties; a `tr` accepts a background that paints under its cells; a `table` accepts `width`, vertical margins, and `break-inside: avoid`. Text wraps inside a narrow cell rather than clipping.

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

Lists use `ul`, `ol`, and `li`. A `ul` or `ol` takes only `li` children, and an `li` belongs in one. Browser defaults apply: 40px of left padding, `1em` vertical margins that drop to zero in a nested list, and `list-style-type` `disc` for a `ul`, then `circle` and `square` one and two lists deeper, and `decimal` for an `ol`. `list-style-type` takes `disc`, `circle`, `square`, `decimal`, `decimal-leading-zero`, `lower-alpha`, `lower-latin`, `upper-alpha`, `upper-latin`, `lower-roman`, `upper-roman`, and `none`. `list-style-position` takes `outside`, the default, or `inside`. The `list-style` shorthand sets both and, as in CSS, resets a part it omits, so `list-style: inside` on an `ol` brings back `disc`. `ol` takes `start` and `reversed`, and `li` takes `value` in an `ol` only. A number outside its counter's range, such as zero in `lower-alpha` or 4000 in `lower-roman`, falls back to decimal. The marker belongs to the first line of the item's first paragraph and moves with it across pages. An outside marker hangs left of the item's text, right-aligned against it, in the item's font, size, and colour; an inside marker starts the first line. Bullets are shapes the engine draws, not glyphs, so they need no glyph in the font and work under PDF/A; counters are text, so their digits and letters must exist in the font. An item that begins with a nested list, table, or image puts its marker on a line of its own. `list-style-image`, string markers, and other counter styles reject. In tagged mode a list item is a paragraph and its counter is a second one: there is no `L`, `LI`, `Lbl`, or `LBody` structure yet.

`dl`, `dt`, `dd`, `blockquote`, `address`, `figure`, `figcaption`, `nav`, and `aside` are blocks with the browser defaults: `blockquote` and `figure` have `1em 40px` margins, `dl` has `1em` vertical margins, `dd` indents 40px, and `address` is italic. Vertical margins do not collapse, so two `blockquote`s in a row are `2em` apart, where a browser leaves `1em`.

`pre` is a monospace block with `1em` vertical margins and `white-space: pre`. `white-space` takes `normal`, `nowrap`, `pre`, `pre-wrap`, and `pre-line`, and anything else rejects. `pre` and `pre-wrap` keep spaces and newlines, `pre-line` keeps newlines and collapses spaces, and `nowrap` collapses whitespace and never wraps. A tab moves to the next multiple of 8 characters from the line start; the count is in characters, so a proportional font only approximates it. A `pre` or `nowrap` line wider than its container rejects with `LineTooWide` and the element path, where `pre-wrap` wraps it. One newline right after `<pre>` is dropped, as in HTML, so a template literal can start on its own line. The engine collapses ASCII spaces, so `pre` and `pre-wrap` keep them as U+00A0: a font without that glyph shows its space glyph, and copied text has no-break spaces. A paragraph record holds 64 KiB, which a `pre` of about 800 average lines fills; split a longer listing into several `pre` blocks with `margin: 0`.

`small`, `sub`, and `sup` are `0.83em`. `sub` and `sup` shift the baseline down by a fifth or up by a third of the parent's font size, and nested ones add up. `vertical-align` takes `baseline`, `sub`, and `super` on inline elements; it rejects on blocks and table cells, on an inline element that has box styles, and with any other value. `u` and `ins` underline, `s` and `del` strike through, `code`, `kbd`, and `samp` use the `monospace` family, and `mark` is black on yellow. A `mark` background paints behind its text, one rectangle per line segment covering the font's ascent and descent; it is the only inline element whose background does not make a layout node.

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

Tailwind's example builds only the utilities used by the PDF template. StyleX passes Babel metadata through `processStylexRules`. styled-components' server output also contains hydration metadata with unsupported attribute selectors and `content`; its example selects the generated component rule before calling `render`. FlashPDF adds no runtime adapter for any of them.

Unsupported declarations fail at `render` only for rules that match an element, naming the property, selector, and source position. `stylesheet(source)` validates the properties and values of every rule with a supported selector up front; a value that uses `var()` resolves only in an element's cascade, so `render` checks it. At-rules and unsupported selectors (`*`, `:root`, pseudo-classes, attribute selectors, escaped names) are skipped:

```
unsupported CSS property: border-radius (box corners are square) in selector ".total" at 4:3
```

Every runtime error names the element and its ancestors, for example `on <span.total> in <div.row> > <main.invoice>`. FlashPDF never drops a declaration that applies to an element it renders.

## Supported subset

- Tags (case-insensitive, as in HTML), `.class`, `#id`, compound, descendant, and child selectors.
- Specificity as an (ids, classes, tags) tuple, then source order across the whole `stylesheets` array, so a later sheet wins a tie. Inline styles beat both, and `!important` declarations beat all non-important ones. A shorthand and its longhands cascade in that order too, so a later `border: none` clears an earlier `border-bottom`, and a later `background` replaces `background-color`. An inline value of `undefined` is skipped, as React does.
- Inherited text properties and `var(--name, fallback)`. The fallback may hold functions and a nested `var()`, such as `var(--c, rgb(0, 0, 0))` or `var(--a, var(--b))`.
- `pt`, `px`, `em`, `rem`, unitless values where accepted, and percent widths in flex rows.
- Block flow, flex rows/columns, `gap` between flex row items and between column children, page size, and document margins.
- `line-height` as `normal` (for the built-in fonts, the browser's: Helvetica and Times are 1.15em and Courier is 1.133em, the hhea ascent, descent, and line gap of Arial, Times New Roman, and Courier New, with the line gap split above and below the text; an embedded font uses its own metrics), a unitless factor, a percent, or a length. A factor inherits as a factor; a percent or `em` resolves against the element's own font size and inherits as a length. The extra height splits evenly above and below the text, as CSS half-leading does.
- `width` and `flex` on the direct children of a `flex-direction: row` box, which is where columns are measured. Both reject anywhere else rather than being dropped. The `flex` shorthand keeps only the grow factor; grow 0 needs a `width`.
- Margin, padding, solid uniform borders, solid backgrounds, text color, `font-family`, and size. `font-weight` takes `normal`, `bold`, or a number from 1 to 1000, where 600 and up is bold; anything else, `bolder` and `lighter` included, rejects.
- `font-style: normal | italic | oblique`. `oblique` picks the italic face too, and an angle rejects. The built-in fonts have their own italic faces: Helvetica-Oblique, Times-Italic, and Courier-Oblique, each with a bold version. An embedded family uses its `italic` or `boldItalic` file; without one, FlashPDF slants the upright face by a quarter (about 14 degrees), as browsers do under `font-synthesis`. A bold italic without a `boldItalic` file slants the bold face.
- `font-kerning: auto | normal | none`, kerning by default. Adjacent letters of a word kern when they share a font and size, also across inline elements; pairs next to a space or a CJK character do not. Helvetica and Times use the KPX pairs of Adobe's AFM files for every WinAnsi pair; Courier is fixed-width and has none. An embedded font uses GPOS pair adjustment (lookup type 2, formats 1 and 2, also inside extension lookups) under the `kern` feature of the `DFLT` script, else `dflt` or `latn`, taking the first glyph's x-advance adjustment; without that, its `kern` table's format 0 subtables. FlashPDF reads at most 128 pair subtables per font. Kerning counts when lines break.
- `text-align: left | center | right | justify`. `justify` stretches the spaces and no-break spaces of every wrapped line to the full width; the last line and a line ending in `<br>` stay left-aligned, and a line without a space, such as unspaced CJK, stays left-aligned too.
- `letter-spacing` and `word-spacing` as `normal` or a length, negative allowed, up to 14,400pt either way; percentages reject. Letter spacing follows every character, including the last on a line, as in browsers. Word spacing follows every space and no-break space. Both count when lines break, and both inherit as points, so `0.1em` resolves against the element that declares it. Embedded fonts ignore the PDF word-spacing operator, so FlashPDF positions each space itself.
- `text-transform: none | uppercase | lowercase | capitalize`, applied before layout with locale-independent case mapping, so Turkish dotted i follows the default Unicode rules. `capitalize` uppercases the first letter of each word, where a word starts after anything but a letter, digit, or mark, except an apostrophe, period, colon, or middle dot inside a word: `don't` stays one word and `e-mail` becomes `E-Mail`.
- `text-decoration: none | underline | line-through`, alone or combined, optionally with `solid` and a colour, on any text element, and `text-decoration-color`. Descendants inherit both, and a shorthand without a colour resets it to the text colour. An underline is centred on the font's underline position at its thickness: the built-in fonts' AFM values (-100 and 50 per 1000), or an embedded font's `post` table. A line-through uses the `OS/2` strikeout, or half the AFM x-height. `overline` and the `dotted`, `dashed`, `double`, and `wavy` styles reject.
- `img`: `width` anywhere (`pt`, `px`, `%`) and `height` (`pt`, `px`), which only `img` accepts. PNG at 8 bits per channel: greyscale, RGB, and palette pass through undecoded with `/Predictor 15`; greyscale+alpha, RGBA, and a palette with `tRNS` split the alpha into an `/SMask`; a greyscale or RGB `tRNS` colour key becomes `/Mask`. JPEG baseline and progressive pass through as `/DCTDecode` in greyscale, RGB, or CMYK, with Adobe APP14 CMYK inverted through `/Decode`.
- `white-space: normal | nowrap | pre | pre-wrap | pre-line`, inherited; `vertical-align: baseline | sub | super` on inline elements; and `list-style-type`, `list-style-position`, and the `list-style` shorthand, inherited.
- `a`: `href` as an absolute `http:`, `https:`, or `mailto:` URL, emitted as one `/Link` annotation per line segment or image. `font-family` lists are read in order and the first entry that is a registered `RenderOptions.fonts` family or a built-in name wins; a registered family beats a built-in of the same name. The built-in names are `Helvetica`, `Arial`, `sans-serif`, `system-ui`, and `ui-sans-serif` for Helvetica, `Times`, `Times New Roman`, `serif`, and `ui-serif` for Times, and `Courier`, `Courier New`, `monospace`, and `ui-monospace` for Courier. Names are case-insensitive, and a quoted `'serif'` is a family name, not the generic keyword. Any other entry, such as `Georgia` or `Inter`, is skipped for the next one, as in a browser. A list where no entry matches rejects with `unregistered font family`, naming the list, so a typo or a forgotten `fonts` entry never silently changes the typeface; end a list with `serif`, `sans-serif`, or `monospace` to always match. Colours accept hex, `rgb()`, `hsl()`, and CSS named colours; opacity rejects.
- `h1`-`h6` defaults of `2em`/`1.5em`/`1.17em`/`1em`/`0.83em`/`0.67em` and bold, which a matching rule or inline style overrides but an inherited `font-size` does not.
- `break-before`, `break-after`, and `break-inside: avoid`.
- Tables: `width` and `flex` on the first row's `th`/`td` size the columns; `padding`, `border`, `background`, and text properties on cells; `background` on `tr`; `width`, `margin-top`, `margin-bottom`, and `break-inside: avoid` on `table`; `break-inside: avoid` on `tbody` keeps its rows together.

Normal block children stream independently, so long `<main>` documents paginate without retaining the whole layout. Flex rows and `break-inside: avoid` groups remain atomic. A single open container may buffer at most 16 MiB of layout commands, and a document may paint at most 16 MiB of content.

## Intentional v1 limits

- Helvetica, Times, and Courier, each in regular, bold, italic or oblique, and bold italic, are the built-in fonts, and Helvetica is the default. Registered host-provided TTF families support regular and optional bold, italic, and bold italic files. Variable-face selection, OpenType features other than kerning, shaping, and `text-decoration: overline` reject.
- Text in an embedded TTF renders any character the font has a glyph for, including CJK and supplementary-plane characters; a character without a glyph rejects, naming it and its code point. There is no shaping, ligature substitution, or bidi: each character maps to one glyph, combining marks render as spacing glyphs, and right-to-left text comes out in logical order. The built-in faces are WinAnsi standard fonts, so non-Latin text needs an embedded font. They have no embedded program, so PDF/A rejects all twelve. Lines break at ASCII whitespace and between CJK characters; U+00A0 does not break.
- Images: 16-bit, 1/2/4-bit, and interlaced PNGs, 12-bit, lossless, hierarchical, and arithmetic-coded JPEGs, and every other format (GIF, WebP, SVG) reject naming the feature. A PNG with transparency must decode to at most 64 MiB, and all images in one render are capped at 64 MiB of input. `object-fit` rejects: an image always scales to its box. An image wider than its container or taller than an empty page rejects with the element path. Image bytes are not CRC-checked; a corrupt file produces a corrupt picture, not an error.
- Links: internal `#anchor` links are not supported yet, and `javascript:`, relative, and non-ASCII-after-encoding URLs reject. Link rectangles have no border. Footer text takes no links or decoration.
- No border radius, gradients, `height` outside `img`, `min-width`, or `max-width`. `border-top`, `border-right`, `border-bottom`, and `border-left` accept the same solid syntax as `border`; `<hr>` renders a one-point black bottom rule, which its own border declarations override; it takes `break-before` and `break-after`, and `width` only as a flex row child.
- Tables have no `colSpan`/`rowSpan`, `colgroup`/`col`, or `caption`. `vertical-align` rejects on cells: they align to the top. `border-collapse` and `border-spacing` reject: cells tile the row with separate borders, so adjacent full borders double up; draw one edge per cell (`border-bottom`, and `border-left` on all but the first column) for single rules. `table` takes no padding, border, background, or horizontal margins; style its cells or a wrapping `div`. A table inside a decorated box, a flex row, or a `break-inside: avoid` group is atomic like any other content there.
- A decorated box is atomic. Use document margins for page-wide padding and decorate individual cards/sections, not a multi-page root wrapper. Rejected at-rules and unmatched rules are skipped silently; `@layer` block contents are skipped too, so import `tailwindcss/utilities` rather than the full preflight.
