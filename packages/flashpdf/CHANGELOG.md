# Changelog

## Unreleased

### Added

- The `tw` JSX prop and HTML attribute map Tailwind v4 default utilities in the Rust core without a build step. Supported spacing, sizing, flex, typography, colours, borders, radius, and arbitrary values follow Tailwind utility order; `style` wins conflicts. Unknown utilities and variants reject by name. The protocol version moves to 26.

- OpenType fonts with CFF outlines (`OTTO`), name-keyed or CID-keyed, such as Source Sans and Noto Sans CJK. FlashPDF embeds them as CIDFontType0 with a `CIDFontType0C` font file, using its own CFF subsetter. The subsetter keeps glyph ids, empties unused charstrings and subroutines, and turns a name-keyed font CID-keyed with an identity charset. CFF2 rejects at registration with a message naming it. The subsetter adds about 23 KB raw and 9 KB gzipped to the WASM, so the size gate rises to 1,600,000 B raw and 650,000 B gzipped.
- Tailwind v4's own layered build renders: `@import "tailwindcss/theme.css" layer(theme)` with the utilities in `layer(utilities)` gives the same bytes as the `theme(inline)` build. The core now reads `@layer` blocks and orders them as CSS does, unlayered rules last and `!important` reversed, evaluates `@supports` against what FlashPDF renders, resolves CSS nesting with `&` and nested `@media`, `@supports`, and `@layer`, and mixes opaque colours with `color-mix(in srgb | oklab, ...)`. Vendor-prefixed declarations are skipped, and an attribute selector on an attribute no element carries, such as `[hidden]` or Tailwind's `data-*:` and `aria-*:`, never matches instead of rejecting. Preflight still rejects on `font-weight: bolder`, `font-feature-settings`, and `text-indent`. The undefined-variable message drops its note about skipped layers. The WASM grows about 25 KB raw and 11 KB gzipped.
- `render` takes an HTML string as the document, and `header`, `footer`, `firstPageHeader`, and `firstPageFooter` take one too. The Rust core parses it into the same elements as JSX, so the cascade, compile, and messages are shared, and the bytes match the same tree written in JSX. `<style>` elements join the cascade after `stylesheets`, and `<img src>` names an entry in the new `images` option. The parser takes optional end tags, an implied `tbody`, and the document wrappers, and rejects anything a browser would repair, such as a misnested end tag or a bare `&`, with its line and column. The WASM grows about 34 KB raw and 15 KB gzipped.
- `render` documents and types the plain `{ type, props }` element objects that JSX compiles to, so a tree parsed from JSON renders without JSX. The new `ElementData` type checks the tag and its attributes. Such a tree takes the same checks and error paths as JSX.
- Variable TrueType fonts. `font-weight` takes any number on a `wght` axis, `font-stretch` sets `wdth`, `font-style: oblique <angle>` sets `slnt` and `italic` sets `ital`, and the new `font-variation-settings` sets any axis by its tag. A tag the selected face lacks rejects, naming its axes. A family with only a variable regular file now renders bold from its `wght` axis instead of rejecting. Each set of axis values embeds as a static TrueType subset with the `gvar` deltas applied in fixed point, `HVAR` or phantom-point advances, and `MVAR` line metrics, under a `/BaseFont` that names the values; HarfRust shapes at the same coordinates. FlashPDF reads the variation tables with `read-fonts`, which HarfRust already depends on; the WASM grows by about 52 KB raw and 22 KB gzipped.
- CSS positioning: `position: relative | absolute | fixed`, `top`, `right`, `bottom`, `left`, `inset`, `inset-inline`, `inset-block`, and `z-index`. A relative box paints offset from its place in flow. An absolute box sits in the padding box of its nearest positioned ancestor, or the page area, out of flow. A fixed box paints in the page area of every page. Positioned boxes paint after the page's normal flow in `z-index` order, and negative ones under it. `sticky`, and absolute or fixed boxes with an axis whose insets are both `auto`, reject.
- CSS `direction: ltr | rtl`, the `dir` attribute (`ltr`, `rtl`, `auto`) on any element, and `text-align: start | end`. `start` is the new default and stays left in left-to-right text. A paragraph's base direction now comes from `direction` and `dir`, `ltr` by default as in CSS, and only `dir="auto"` takes it from the first strong character. A right-to-left paragraph aligns right by default, and a justified one ends its last line on the right.
- `flex-wrap: wrap` on a flex row: items fill each row until the next one would pass it, and every row flexes and justifies its items on its own. `row-gap` and `column-gap` set the gaps separately, and `gap` takes one or two lengths, row gap first. `align-content` shares a row container's spare height among its rows. Without a height, a wrapping row breaks across pages between its rows. `wrap-reverse` and a wrapping flex column reject.
- Font fallback: a character the run's font lacks is drawn, with its combining marks, by the first registered font that has the whole cluster, faces of the same weight and slant first in registration order, instead of rejecting. Built-in fonts fall back the same way for characters outside WinAnsi. A character no registered font has still rejects with `MissingGlyph`, or `UnsupportedCharacter` from a built-in font.
- Right-to-left text: a paragraph that holds Hebrew, Arabic, or a right-to-left embedding, override, or isolate gets Unicode UAX #9 levels around its base direction. Each line shows its runs in visual order after breaking, and right-to-left words shape right to left, so Arabic joins and brackets mirror. Paragraphs without right-to-left text skip the algorithm. FlashPDF depends on the `unicode-bidi` crate for this.
- Node, Bun, and Deno render through a native N-API addon when a prebuilt one exists for the platform, and through the WASM otherwise. The bytes are the same either way. The addon skips WASM compilation, so on the 80-line benchmark invoice the first render in a fresh process takes roughly half as long; warm renders gain less while the TypeScript cascade and compile dominate them. The release publishes one `@pettersen3008/flashpdf-<platform>` package per platform as an optional dependency. The loader moves from the `#wasm` to the `#backend` imports map, with `bun`, `deno`, and `node` conditions.
- The native build deflates page streams, subsets fonts, and splits PNG alpha planes on several threads once a document holds 64 KiB of such work. Output order is fixed, so the bytes match a single thread.
- Deno 2 is a tested runtime.
- Embedded fonts are shaped with HarfRust 0.9 and the font's default OpenType features: ligatures, contextual alternates such as Inter's raised colon between digits, positioned combining marks, and GPOS or `kern` table kerning. `/ToUnicode` maps a ligature to all its letters and a private-use alternate to its character, and tagged output marks a cluster its glyphs cannot spell with `/ActualText`. Letter spacing turns `liga` and `clig` off. A shaped word is cached per font and reused while the same font bytes stay registered, so warm renders of the 80-line Inter invoice are no slower than before. The first render in a process shapes every word: about 5 ms more natively and 13 ms more in WASM, which is also slower to compile. The built-in fonts keep their AFM widths and kerning. HarfRust adds about 489 KB raw and 193 KB gzipped to the WASM; versions after 0.9 read fonts through a model that adds about 480 KB more.
- `pdfa` takes `'2b'` and `'2u'` as well as `'3b'`. PDF/A-2 allows only embedded files that are PDF/A themselves, so `attachments` and `facturx` reject with it and keep needing `'3b'`. veraPDF 1.30 passes the 2b, 2u, and 3b output, tagged and untagged.

### Changed

- Lines break at the Unicode UAX #14 break opportunities instead of only at ASCII whitespace and between CJK characters: after a hyphen or `/`, never before closing punctuation, so a long URL or hyphenated word wraps at a natural point instead of mid-glyph. U+00AD shows as `-` only where a line breaks there, U+200B breaks invisibly, and U+2060 keeps text together. FlashPDF depends on the `unicode-linebreak` crate for this, which adds about 23 KB to the WASM.
- The Rust cascade and compile take about 68 KB less WASM: compile and CSS errors are boxed, so each `?` moves one pointer; small helpers called from hundreds of places, such as style lookups, value comparisons, and record byte writers, stay out of line; and `wasm-opt` runs with `--low-memory-unused`. The WASM drops from 1,393 KB to 1,324 KB raw and from 576 KB to 556 KB gzipped. Messages and PDF bytes do not change, and warm renders are no slower.
- The WASM size gate is 1,600,000 B raw and 650,000 B gzipped, up from 400 KB and 200 KB, to leave room for CSS positioning (36 KB raw, 16 KB gzipped), Unicode line breaking, the Rust cascade and compile, shaping, and bidi.
- The cascade, style normalisation, and element compile run in the Rust core instead of TypeScript. JavaScript resolves the JSX tree and sends it to the core as element records; the PDF bytes do not change. On the 80-line benchmark invoice a warm render takes about 4 ms instead of 6 to 7 ms in Bun with the native addon, and 4.6 ms instead of 6.8 ms in Node through the WASM. The core parses each stylesheet once per process and reuses it while the text stays the same, and tests each element only against the rules whose last compound names its id, a class, or its tag. The WASM grows from 397 KB to 853 KB raw, 171 KB to 363 KB gzipped.
- The core checks fonts, page options, `@page`, metadata, PDF/A, and attachments before user components run. Engine errors name the element that caused them without rendering twice.
- A style object value for a supported property must be a string or a number; anything else fails with `style <name> must be a string or number, not <type>` and the element path.
- `stylesheet()` no longer throws. It returns the source and marks it, and the first `render` that uses the sheet runs the full check in the Rust core and rejects with the same message, such as `unsupported color: notacolor in selector ".a" at 1:1`. Every later render of the same text skips the check once it passed. Code that caught the error around `stylesheet()` must catch it from `render` instead.
- Deflate settings depend on what a stream holds: page and form content at level 5 with the filtered strategy, font programs, CMaps, and object and cross-reference streams at level 4, images and attachments at level 6 as before. Content and font deflates take about a third less time; on the 80-line benchmark invoice in Bun a tagged warm render drops from about 2.9 ms to 2.7 ms native and 3.45 ms to 3.15 ms WASM, an untagged one by up to 0.1 ms. The file grows 0.5% untagged (13,389 to 13,453 B) and 2 B tagged. The output buffer is sized from the deflated streams instead of the full font files.
- Each font's deflated subset is kept with its shaping cache, at most four glyph sets per font, so a render that shows the same glyphs as an earlier one skips subsetting and deflating the font. A cold subset marks kept glyphs in a bit set instead of a tree and copies each table once, which cuts its time on Inter from about 80 µs to 33 µs, and `/ToUnicode` finds each glyph's code point in about a quarter of the time. On the 80-line benchmark invoice in Bun with the native addon, a warm render drops from about 2.5 ms to 2.25 ms untagged and 2.85 ms to 2.6 ms tagged. The bytes do not change; the WASM grows 2.4 KB raw and 0.4 KB gzipped.
- Tagged output is about a third of its old size: the structure tree goes compact into one Flate object stream, each element names its marked content as `/K <mcid>`, and a cross-reference stream replaces the xref table. The 80-line invoice drops from 52.3 KB to 17.2 KB. Untagged output is unchanged.

### Fixed
- An `<img>` with auto width and height and a `maxHeight` equal to the page height no longer throws PageOverflow. Scaling into a max now clamps the result to the max, so `maxHeight: 842` on an A4 page without margins draws the image exactly 842 pt tall.

### Internal
- Dependabot ignores all intentionally exact-pinned Rust dependencies, including HarfRust's WASM size pin and pdf-writer's cross-reference format pin.
- FlashPDF's own GPOS and `kern` table reader is gone; HarfRust kerns embedded fonts. HarfRust and `read-fonts` build with `opt-level = "z"`, about 100 KB less WASM, since the word cache keeps shaping off the warm path.

- The Rust core gains a port of the CSS parser, selector matcher, cascade, `var()`, `calc()`, style normalisation, and the element compiler, with the TypeScript tests for them ported to Rust. `compile.ts`, the TypeScript page setup, and the TypeScript drawing encoder are gone. The TypeScript CSS parser, cascade, and style modules are gone too; only the SVG colour parser stays in TypeScript.
- The record protocol is version 24. The header is the magic and version; a Page record carries the page size for layout records, and Family, Stylesheet, StrictStylesheet, Setup, Block, Open, Text, Close, and Drawing records carry the element tree. `<img>` bytes are staged with `stage_image` and register as images when first painted, so image order in the PDF stays the document order.
- `pdf-writer` builds with `opt-level = "z"`, which cuts the WASM by about 23 KB with no measured change in warm render time. `build.sh` prints the brotli size when `brotli` is on PATH.

## 0.5.0 - 2026-10-27

### Added

- `max-width` and `max-height` on `img` and `svg`. With `width` and `height` both auto the image scales into both caps and keeps its aspect ratio, as CSS 2.1 §10.4 says, so a scan of unknown size fits the page instead of rejecting. A cap never enlarges an image, and a percent `max-height` rejects like a percent `height`. The layout protocol moves to version 22.

### Fixed

- The README and CSS.md each described images twice, and the older copy left out SVG and `#id` links.
- `render` resolves to `Uint8Array<ArrayBuffer>`, so `new Blob([pdf])` type-checks without a cast. The type needs TypeScript 5.7 or later.
- `typesVersions` maps `jsx-runtime` and `jsx-dev-runtime`, so TypeScript with `moduleResolution: "node"` finds the JSX types without a `paths` entry.
- The README states that the built-in fonts cover WinAnsi only, and that text from users needs an embedded font.

## 0.4.0 - 2026-10-20

### Added

- Inline `<svg>` for logos, icons, and charts: `g`, `rect` with `rx`/`ry`, `circle`, `ellipse`, `line`, `polyline`, `polygon`, and `path` with the full `d` grammar, arcs included. Elements take `transform`, the fill and stroke presentation attributes or a `style` object, opacity, and `currentColor`. The drawing lays out like `img`, honours `viewBox` and `preserveAspectRatio`, clips to its box, and in tagged mode becomes a figure described by `aria-label` or `<title>`. Text, gradients, patterns, filters, masks, `clipPath`, `use`, `image`, `foreignObject`, CSS inside the drawing, and unknown attributes reject with a reason.
- `<img src={bytes}>` accepts an SVG file with the same subset.
- `QRCode` draws a QR code as vector paths: numeric, alphanumeric, or UTF-8 byte mode, error correction L, M, Q, or H (default M), the smallest version from 1 to 40, the standard's mask selection, and a quiet zone in modules. Children draw over the symbol, for a logo or the Swiss cross. The encoder is a port of Project Nayuki's MIT-licensed QR Code generator.
- `Barcode` draws Code 128 with automatic code set B and C switching and the checksum, a module width and bar height in points, and the value printed below unless `text={false}`.
- `border-radius` and its four corner longhands, with one or two lengths or percents per corner. The background fills the rounded shape and a border with one width, colour, and style on all four sides follows it. Radii that overrun a side shrink together, so `9999px` and Tailwind's `rounded-full` make a pill. A radius with differing sides, and a radius on `img` or `svg`, reject with a reason. Rounded boxes move whole to the next page so a page break does not cut their corners.
- `border-style` and the four side longhands, plus the style word in `border` and its side shorthands: `none`, `hidden`, `solid`, `dashed`, `dotted`. Dashes follow Chrome's proportions, dots are round-capped, and each side stretches its gaps so whole dashes fill it. `double`, `groove`, `ridge`, `inset`, and `outset` reject. Tailwind's `border-dashed` and `border-dotted` work.
- `opacity` on boxes, text, underlines, images, and drawings, as a number or percent, multiplied down the tree. Each element draws with its own graphics-state alpha instead of a transparency group, so a translucent card's background shows through its text. PDF/A-3b output stays valid; veraPDF accepts it.

- `oklch()` and `oklab()` colours, converted to sRGB and clipped per channel. Tailwind v4 emits them for every colour utility.
- `inherit` and `initial` on every supported property, and `currentcolor` as the element's own `color`.
- `text-decoration-line`, with the values `text-decoration` takes, so Tailwind's `underline` and `line-through` work.
- `rem` for `width` and image `height`, as minified Tailwind emits for `w-32`.
- `calc()`, `min()`, `max()`, and `clamp()` over lengths and plain numbers, evaluated after `var()` substitution, so Tailwind's `calc(0.25rem * 4)` spacing renders. A percentage result works for `width`, `height`, and `line-height`; a percentage mixed with a length rejects with a reason.
- Logical `margin-inline`, `margin-block`, `padding-inline`, `padding-block`, and their `-start` and `-end` longhands, mapped for horizontal left-to-right text. Tailwind's `px-*`, `py-*`, `mx-*`, and `my-*` use them.
- `border-style` with `solid`, `none`, and `hidden`, and per-side `border-*-style`, `border-*-width`, and `border-*-color`. Other styles reject.
- `@property`: `initial-value` is the default for an unset custom property, and `inherits: false` stops inheritance. Tailwind's `border` utility needs it for `--tw-border-style`.
- Escaped selectors such as `.w-1\/2` and `.w-\[120px\]` match the unescaped class name.
- `@media`: `print` and `all` apply, `screen` is skipped, and width conditions compare the page content width. Any other query rejects with a reason where a rule inside it matches an element.
- Structural selectors: `:first-child`, `:last-child`, `:only-child`, `:nth-child()`, `:nth-last-child()`, the five `-of-type` forms, `:empty`, `:root`, and `*`. Siblings are the element children of a parent on the resolved tree, so text, fragments, and arrays do not count.
- `:is()`, `:where()`, and `:not()` with selector lists, complex selectors included, and the next-sibling `+` and subsequent-sibling `~` combinators. `:is()` and `:not()` take their most specific argument and `:where()` adds nothing.
- Tailwind v4's `first:`, `last:`, `only:`, `odd:`, `even:`, `empty:`, `nth-*:`, `*:`, `space-y-*`, and `divide-y` utilities render. They compile to these selectors, and FlashPDF skipped them before.
- `colSpan` and `rowSpan` on `td` and `th`. A spanning cell takes the summed widths of its columns, and each column takes its width from its first cell without `colSpan`, so an invoice's "Subtotal" can span the description columns. Rows that a `rowSpan` joins stay in one row group and move between pages together; the last row grows when the spanning cell needs more height.
- `vertical-align: top | middle | bottom` on `td` and `th`, and on `tr` and row groups for their cells. `baseline` rejects.
- `border-collapse: collapse` draws one line between neighbouring cells: the wider border wins, and on a tie a cell's beats its row's. `border-collapse: separate` and `border-spacing: 0` describe what tables already draw; other spacing rejects.

- `RenderOptions.pdfa: '3b'` writes PDF/A-3b: XMP metadata that repeats `metadata`, an sRGB output intent, and a document ID hashed from the content. In this mode, text that would use the unembedded Helvetica and CMYK JPEGs reject.
- `RenderOptions.attachments` embeds files with an `AFRelationship`, listed in the catalog's `/AF` array and the EmbeddedFiles name tree. Names, MIME types, and relationships are validated, with a 64 MiB cap per render.
- Internal links: `<a href="#id">` jumps to the element with that `id`, on whatever page it lands. The id must be on one element, and a link to a missing or repeated id rejects naming it.
- Named destinations: every element a link targets is in the `/Dests` name tree, so `invoice.pdf#nameddest=terms` opens there in a viewer.
- `RenderOptions.outline` writes bookmarks for `h1` to `h6`, nested by level and titled with the heading's text. `data-bookmark="Title"` renames a heading's bookmark and `data-bookmark={false}` skips it.
- `RenderOptions.facturx: { xml, profile }` embeds Factur-X or ZUGFeRD XML as `factur-x.xml` (`xrechnung.xml` for `XRECHNUNG`) with the Factur-X XMP extension schema, and implies `pdfa: '3b'`. FlashPDF does not validate the XML content.
- Italic text: `font-style: italic | oblique`, and `em`, `i`, and `cite` default to italic. Helvetica gains Helvetica-Oblique and Helvetica-BoldOblique, `fonts` entries take optional `italic` and `boldItalic` files, and a family without the italic file for a weight slants its upright face by a quarter, as browsers do.
- `letter-spacing` and `word-spacing`, negative values included. Both count when lines break. Embedded fonts get word spacing through `TJ` positioning, because PDF word spacing skips their two-byte codes.
- `text-align: justify`. Wrapped lines stretch their spaces to the full width; the last line and a line ending in `<br>` stay left-aligned.
- `text-transform: uppercase | lowercase | capitalize | none`, with locale-independent case mapping.
- `text-decoration-color`, and a colour in the `text-decoration` shorthand.
- Kerning, on by default, with `font-kerning: auto | normal | none`. Helvetica uses Adobe's AFM pairs; embedded fonts use GPOS pair adjustment under the `kern` feature, or the `kern` table. Line breaking measures kerned widths.
- Times and Courier built-in fonts, each in regular, bold, italic or oblique, and bold italic, with Adobe's AFM widths and, for Times, its kerning pairs. `font-family` takes `serif`, `Times`, and `Times New Roman` for Times, and `monospace`, `Courier`, and `Courier New` for Courier. Output of a document that uses neither is unchanged.
- Lists: `ul`, `ol`, and `li`, with `list-style-type` (`disc`, `circle`, `square`, `decimal`, `decimal-leading-zero`, `lower-alpha`, `lower-latin`, `upper-alpha`, `upper-latin`, `lower-roman`, `upper-roman`, `none`), `list-style-position`, the `list-style` shorthand, `ol` `start` and `reversed`, and `li` `value`. A `ul` nested one and two lists deep uses circles and squares, as browsers do. Bullets are shapes the engine draws, so they need no font glyph, and a marker stays with its item's first line across pages.
- `dl`, `dt`, `dd`, `blockquote`, `address`, `figure`, `figcaption`, `nav`, and `aside` as blocks with the browser's default margins, indent, and italic.
- `small`, `sub`, `sup`, `u`, `ins`, `s`, `del`, `mark`, `code`, `kbd`, and `samp`, with `vertical-align: baseline | sub | super` on inline elements. `mark` paints a yellow background behind each line of its text.
- `pre` and `white-space: normal | nowrap | pre | pre-wrap | pre-line`, with tabs to the next multiple of 8 characters. A `pre` or `nowrap` line wider than its container rejects with `LineTooWide`.

- `line-height`: `normal`, a unitless factor, a percent, or a length. A factor inherits as a factor and scales with each descendant's font size; a percent or `em` resolves against the element's own size and inherits as points. The extra height splits evenly above and below the text, as CSS half-leading does.
- `gap` on a flex row. The gap is a fixed-width empty column between items, so `flex` grow shares the remaining width.
- `tfoot` rows repeat after the last body row of every page a table spans, and their height is reserved on every page. Previously they painted once at the end.

### Changed

- `box-shadow`, `outline`, `border-image`, and `background-image` reject with a reason, as does a gradient in `background`. They rejected without one before. A colour with alpha now says to use `opacity`.
- `border: 1pt dashed red` and `border-style: dashed` no longer reject. `<br>` rejects `opacity` like its other box properties. Rich headers and footers support opacity.

- Paragraphs, headings, and horizontal rules use Chromium's default vertical margins, and table headers centre their text. Author styles override these defaults, and adjoining margins collapse. Existing documents that relied on zero paragraph or heading margins should set `margin: 0`.
- Vertical margins collapse as in CSS 2.1. Adjacent siblings, a box and its first or last child with no border or padding between them, and an empty block share the largest margin instead of adding them up, so two `blockquote`s in a row are `1em` apart, not `2em`. Flex items and table cells keep their margins. Documents with adjoining vertical margins get less space between blocks.
- A margin that does not fit at the bottom of a page no longer moves to the top of the next one, and a forced page break drops the margin before it. Before, such a margin could push the next block a page further or leave a blank page.
- The engine compiles for speed instead of size (`opt-level = 3`, `wasm-opt -O3`). Layout and PDF writing in WASM run about 32% faster; the WASM grows from 241 KB to 295 KB raw (114 KB to 125 KB gzip). Output bytes are unchanged.
- A styled 10,000-row invoice renders about 68% faster (630 ms to 200 ms). Elements with the same parent style, tag, id, classes, and inline style share one computed style; box styles and colours are parsed once; stylesheets are parsed once per render instead of once each for the body, header, and footer. Protocol records go to WASM up to 64 KiB per batch instead of one call per record, and `PdfRenderer.records()` reports which record the engine rejected, so errors still name the element that caused them. The protocol framing and output bytes are unchanged.
- `render` resolves `header` and `footer` components before it registers fonts and attachments, so an error in one of them now comes before a font or attachment error.
- A rule with an escaped selector, such as Tailwind's `.bg-black\/50`, now matches and validates instead of being skipped, so `stylesheet()` rejects one whose declarations the subset lacks.
- Page setup: `pageFormat` takes `A3`, `A4`, `A5`, `Letter`, `Legal`, or `{ width, height }`, `orientation: 'landscape'` puts the longer side across, and `margin` takes one to four lengths in CSS shorthand order or `{ top, right, bottom, left }`. The body, header, and footer follow all four margins.
- `@page { size; margin }` in stylesheets, with the four margin longhands. `size` takes `auto`, a named size with an optional orientation, or one or two lengths. Render options win over `@page`.
- `mm`, `cm`, `in`, `pc`, and `Q` lengths wherever a length or width is accepted, converted at 72pt per inch.
- `header` and `footer` take any element tree the body takes: several lines, bold and coloured runs, links, flex rows and columns, images, tables, borders, and backgrounds. `PageNumber` and `TotalPages` work anywhere inside them. Each page lays its header and footer out again with its own numbers; the reserved height counts every number as five digits.
- `firstPageHeader` and `firstPageFooter` replace the header and footer on the first page, and `null` leaves the first page without one. Each page reserves the height of the header and footer it paints.
- Flex alignment in rows and columns: `justify-content` (`flex-start`, `flex-end`, `center`, `space-between`, `space-around`, `space-evenly`), and `align-items` and `align-self` (`stretch`, `flex-start`, `flex-end`, `center`), plus `start`, `end`, and `normal`. Tailwind's `justify-between` and `items-center` work. A column needs a `height` or `min-height` for `justify-content`.
- Flex item sizing in rows: `flex-grow`, `flex-shrink`, `flex-basis` in `pt`, `px`, `rem`, or `%`, and every `flex` shorthand form, resolved as CSS does. An item shrinks no further than its longest word or image unless `min-width` allows. `auto` margins on items take the free space, so `ml-auto` pushes an item right.
- Box sizing: `width`, `min-width`, `max-width`, `height`, `min-height`, and `max-height` on blocks, paragraphs, headings, and `hr`; `box-sizing: border-box`; and `margin-left` and `margin-right: auto`, which centre a block with a `width` or `max-width`. A percent height resolves against the parent's fixed height. A box with a `height` or `max-height` moves whole to the next page when it does not fit, and content taller than it rejects with `ContentOverflow`.

- A flex row keeps the earlier column model, where an item without `flex` or `width` takes an equal share, until the row or an item uses a property above, so existing documents render unchanged. `flex: auto` beside a `width`, a `flex` length basis, and `flex: none` without a width no longer reject; they switch the row to the CSS algorithm.
- `flex-wrap: wrap` and `align-items: baseline` reject with a reason, as do `justify-content` and `align-items` outside a flex container, and `flex-grow` or `flex-basis` on a column item.

- A page's header and footer may take at most half the page between its margins; more rejects with `PageOverflow` naming the option. Headers and footers lay out like body text, so whitespace collapses, and a right-aligned line can move by under 0.0001pt.
- `@page` rules apply instead of being skipped. Page selectors such as `:first`, margin boxes such as `@top-center`, and properties other than `size` and the margins reject with a reason.
- A box with padding, a border, a background, or side margins splits across pages, like CSS `box-decoration-break: slice`. The first page draws its top border and padding, the last page its bottom ones, and every page its side borders, side padding, and background; the content continues below the page header. Nested boxes split too, and a table inside one streams its rows and repeats its header on every page. Before, such a box failed with `PageOverflow`, so an invoice wrapped in `<main style={{ padding: 24 }}>` could not pass one page.
- The engine streams a box's children as they arrive, so memory stays near one page. A 10,000-row table in a padded, bordered card renders 176 pages in 4.6 MB of WASM memory, against 4.1 MB for the same table outside a box; before, it grew to 31.7 MB and failed. The WASM grows by 6 KB raw (4 KB gzip).
- `orphans` and `widows`, inherited and 2 by default: the fewest lines of a paragraph that a page break leaves at the bottom of a page and carries to the top of the next.
- `break-before` and `break-after` take `avoid` and `avoid-page`, which keep an element with the content on the other side of the break: a heading with the first lines of its paragraph, a table row with the next row, or a paragraph's last lines with the block after it. `break-inside` takes `avoid-page`, and `break-inside: avoid` on a paragraph keeps it on one page. When no allowed break point is left, the engine drops `orphans` and `widows` first, then the avoid rules, and it holds back at most one page of content to decide.
- `break-before: page` and `break-after: page` work inside a box with padding, a border, or a background, which continues on the next page. They failed with `invalid nesting` before.

- Pages break as CSS Fragmentation describes, so documents that rendered before can paginate differently:
  - A box with padding, a border, a background, or side margins that does not fit the space left breaks at the page end, as browsers do, instead of moving whole to the next page. It moves only when its first lines do not fit below its top border and padding, so no page ends with a box's top edge alone, and its bottom border and padding stay with its last lines.
  - A paragraph keeps two lines on each side of a break (`orphans: 2; widows: 2`), so a page no longer ends or starts with one line of a paragraph, and a paragraph of three lines or fewer moves whole. `orphans: 1; widows: 1` restores the old line breaks.
  - `h1` to `h6` default to `break-after: avoid`, as print style sheets do (CSS 2.1 Appendix D, Prince, WeasyPrint), so a heading moves to the next page with its first lines instead of ending a page alone. Browsers and the HTML Standard set no break properties on headings; `break-after: auto` restores their behaviour.
  - A margin at an unforced page break truncates to zero. Before, a spacing margin that did not fit moved to the top of the next page, and a box that moved kept its top margin there.
  - A forced break with nothing above it on its page does nothing, so `break-before: page` on the first element no longer adds a blank first page, and `break-after: page` followed by `break-before: page` makes one break. A forced break before a box's first child moves the box along, and one after its last child comes after the box.
  - Borders and backgrounds of boxes outside flex rows, tables, and `break-inside: avoid` groups paint beneath the page's content, as split boxes did, so such documents get new content-stream bytes even where pages break as before. The CSS invoice example keeps its bytes.
- `break-before` and `break-after` reject `left`, `right`, `recto`, and `verso`, naming the reason: pages have no sides.
- Content streams set `rg` and `Tf` only when the page's fill colour or font changes, not again for every line and table cell, and print colours with three decimals instead of float noise such as `0.13333334`. The 10,000-row invoice's uncompressed content shrinks from 3.25 MB to 1.96 MB and the PDF from 288 KB to 265 KB.
- A `PageOverflow` inside a box names the content that does not fit instead of suggesting document margins, and a `break-inside: avoid` box taller than a page says to drop `break-inside: avoid`. `box-decoration-break` rejects with a reason, because a split box always paints as `slice` does.

- `font-weight` rejects values other than `normal`, `bold`, and 1 to 1000, such as `bolder` or a typo. They rendered regular before.
- `<hr>` and `<br>` reject children, `<br>` rejects box and layout styles, and `width` on an `<hr>` outside a flex row rejects. All were dropped before.
- `<td>` and `<th>` outside a table row reject. They rendered as a `<div>` before.
- `stylesheet()` validates declaration values, not only property names, so `.a { color: notacolor }` rejects up front, naming the selector and position. Custom property definitions and values that use `var()`, `calc()`, `min()`, `max()`, `clamp()`, or `color-mix()` are checked at `render` where a rule applies, so a Tailwind v4 build still passes `stylesheet()`.
- Rules whose type selector names a tag FlashPDF never renders, such as `html`, `body`, or `button`, are skipped like other selectors that cannot match.
- A camelCase property name in a stylesheet, such as `fontSize`, rejects with its kebab-case spelling. Property names are now case-insensitive, as in CSS.
- An `http:` or `https:` `href` must spell out its host, so `http:foo` rejects.
- A `flex` grow above 0 with a length basis, or with `auto` beside a `width`, rejects. Columns keep only the grow factor, so the basis was dropped.
- U+001E and U+001F reject in user text. They are the page number and total pages tokens, so they threw in body text and became page numbers in a footer. `PageNumber` and `TotalPages` still emit them.
- A `width` or image `height` too large for a 32-bit float rejects as an invalid width instead of reaching the engine.

- Link annotations carry the print flag, which PDF/A requires.
- A selector that could match an element but is outside the subset rejects at `stylesheet()` and `render`, naming the selector and reason, instead of being skipped. This covers attribute selectors such as `[href]`, `:has()`, `:lang()`, namespaces, unknown pseudo-classes, and malformed selectors. Pseudo-elements, state pseudo-classes such as `:hover` and `:disabled`, `:host`, and tags FlashPDF never renders can never match in a PDF, so they stay skipped. Move a rule that uses `[href]` or `:has()` out of the sheet, or select a class.
- Rules with `*` or `:root` apply instead of being skipped, so an unlayered Tailwind preflight now rejects at its `box-sizing` declaration. Tailwind puts the preflight and theme in `@layer`, which FlashPDF still skips.
- Underlines and line-throughs use the font's metrics: Helvetica's AFM underline position and thickness, or an embedded font's `post` and `OS/2` values. They were 1/14 em thick at fixed offsets before.
- `font-family` reads its list in order and takes the first registered family or built-in name. A name that is neither, such as `Georgia`, is skipped for the next entry, and a list where nothing matches still rejects with `unregistered font family`. `serif` and `ui-serif` select Times, where they selected Helvetica.
- The glyph error for a character outside WinAnsi reads `the built-in font has no glyph for ...`, because Times and Courier share the limit. PDF/A rejects text in any built-in font.
- `ul`, `ol`, and `li` no longer reject as unsupported elements, and `vertical-align` works on inline elements. It still rejects on blocks and table cells.
- A font without a U+00A0 glyph shows its space glyph for it, where it rejected the character. `pre` relies on this.
- `line-height: normal` for the built-in fonts matches browsers. It was the AFM ascender plus descender (Helvetica 0.925em, Times 0.900em, Courier 0.786em), tighter than the metric-compatible Arial, Times New Roman, and Courier New a browser uses. It is now their hhea ascent, descent, and line gap: 1.150em for Helvetica and Times, 1.133em for Courier, with the gap split above and below the text. Every document that uses a built-in font spaces its lines more widely, and header, footer, and link-box heights follow. Embedded fonts keep their own metrics, and an explicit `line-height` keeps its height, with the text about 0.1em lower inside it, where a browser puts it.
- The internal binary protocol is version 21. JavaScript and WASM ship together; adapters must use the matching pair.

### Fixed

- Deferred block margins flush before row, flex, and table boundaries, so they cannot become extra cells. Oversized page furniture errors name the header or footer option.
- Buffered layout frames close without consuming their parent frame. Atomic blocks and table rows respect the different body heights of first and later pages, and split-box backgrounds continue below the header on the next page. Rounded boxes remain whole at page breaks.
- A table's body rows stream: the engine lays out and places each row as it arrives and keeps only the header, footer, and current row. Before, the whole table buffered, so 20,000 padded rows (30,000 plain) failed with `block exceeds 16 MiB`, and 10,000 rows grew WASM memory to 29 MB. 50,000 rows now render in 16 MB. A table inside a flex row or a `break-inside: avoid` group still buffers whole.
- With a header, a line or image taller than the page rejects with `PageOverflow` instead of looping until the renderer runs out of memory.
- Blocks that move whole, such as decorated boxes, rows, and table rows, reject when they do not fit between the header and footer, instead of painting over the footer and into the bottom margin.
- Headers, footers, and repeated table headers count toward the 16 MiB painted-content cap as they paint, so a long footer on thousands of pages rejects with `DocumentTooLarge` instead of growing to gigabytes.
- A line that mixes font sizes or line heights is as tall as its highest ascent plus its lowest depth below the baseline, so it no longer overlaps the next line.

- A PNG with transparency stops inflating at the size its dimensions allow, so a small compressed image can no longer exhaust memory.
- Embedded fonts use one `cmap` subtable: format 12 before format 4, then Windows before Unicode platform. A font with many records or overlapping format 4 segments no longer takes minutes to load, and a format 12 group that starts inside the previous one keeps its remaining characters.
- Registered fonts are capped at 64 MiB per render, checked in JavaScript before the bytes are copied into WASM memory.
- Page sides over 14,400 pt reject, the largest size PDF readers accept.
- A document stops at 10,000 pages with `document exceeds 10000 pages`. JavaScript counts explicit page breaks and the engine counts every page. Pages no longer reserve 1 KB of content buffer each, so 9,999 page breaks take 2.8 MB of WASM memory instead of 14.4 MB.
- Images are capped at 64 MiB per render in JavaScript before the bytes are copied into WASM memory, as fonts are.
- A font or image that fails to register no longer counts toward its 64 MiB per-render cap.

- The package root exports `createElement`. TypeScript and Babel import it for `<Row {...row} key={row.id} />`, a spread before `key`, so such a module failed to load.
- `<hr>` draws its one-point rule. The default border never reached the box, so a plain `<hr>` painted nothing.
- Whitespace-only text between block-level children or flex items, such as Prettier's `{" "}`, no longer adds an empty flex column or a second `gap`.
- A `border` or `background` shorthand in a later rule overrides an earlier `border-bottom` or `background-color`. Previously the longhand always won, whatever the cascade order.
- An inline style value of `undefined` no longer erases the stylesheet's value for that property.
- A `var()` fallback may hold parentheses and nested `var()`, such as `var(--c, rgb(0, 0, 0))`. A malformed `var()` rejects naming the reference.
- A failed WASM load is no longer cached, so the next `render` retries it.
- Stylesheet error positions count the lines inside comments.
- `render` checks its options before it runs any component. Fonts are still checked when they are registered.
- Descendant selectors match in polynomial time. A long selector such as `section div div div div div div div div` on a 40-deep tree took 5.6 s.
- Arrays nested directly in arrays count toward the 64-level nesting limit, so deeply nested arrays reject instead of overflowing the stack. The children array React puts under each element stays free.
- A style object with an own `__proto__` key, as `JSON.parse` makes, rejects instead of skipping validation.
- Type selectors are case-insensitive, so `P { }` matches `<p>`.
- `<hr>` honours `break-before` and `break-after`.
- An engine `InvalidLayout`, `TextTooWide`, or `PageOverflow` now says what to change and names the element.
- `stylesheet()` and `render` no longer fail to parse a sheet whose escaped class name holds a quote, such as Tailwind's `.\[\&_svg\:not\(\[class\*\=\'size-\'\]\)\]`.
- A non-element object child, such as a `Date`, rejects naming its type instead of `unsupported element undefined`.

## 0.3.0 - 2026-09-25

### Added

- Repeating `RenderOptions.header` text with page number and total page tokens. Headers reserve their measured height before body pagination.
- `RenderOptions.metadata` for PDF title, author, subject, keywords, and language.
- Opt-in `RenderOptions.tagged` with a structure tree for paragraph lines and figures, image descriptions from `alt`, and decorative images and page furniture marked as artifacts. This is basic tagging, not PDF/UA conformance.
- Playground TSX can export `{ document, options }` to test render options.
- Images: `<img src={bytes} alt="…" />` with PNG (8-bit greyscale, RGB, palette, greyscale+alpha, RGBA) or JPEG (baseline and progressive, greyscale/RGB/CMYK) bytes. `width` in `pt`/`px`/`%` and `height` in `pt`/`px`; one follows the other by aspect ratio, and neither means the pixel size at 96 px per inch capped to the container. The same `Uint8Array` embeds once per render. Unsupported depths, interlacing, and formats reject naming the feature; 64 MiB of image bytes per render.
- Links: `<a href>` as an inline run or around an image or block, emitted as `/Link` annotations with one rectangle per wrapped line. `http:`, `https:`, and `mailto:` only; `javascript:`, relative URLs, and `#anchors` reject. `a` defaults to `color: #0000EE` and `text-decoration: underline`.
- `text-decoration: none | underline | line-through`; `overline` rejects.
- Unicode text through embedded fonts: any character the TTF has a glyph for renders, including CJK and supplementary-plane characters (cmap formats 4 and 12), as a Type0/CIDFontType2 font with `Identity-H` and a `ToUnicode` map. Lines break between CJK characters as well as at ASCII whitespace. A character without a glyph rejects naming it, its code point, and the element. No shaping, kerning, ligatures, or bidi.
- Tables: `table`, `thead`, `tbody`, `tfoot`, `tr`, `th`, and `td`. Columns come from the first row's `width`/`flex`, rows split across pages with the `thead` repainted on every continuation page, cells stretch to the row height, `th` is bold, and `tr` takes a background. `colSpan`, `rowSpan`, `vertical-align`, `border-collapse`, and `border-spacing` reject with a reason; mismatched cell counts and a row taller than a page reject naming the `tr`.
- Inline rich text: `span`, `b`, and `strong` inside a paragraph become runs that wrap together, each with its own font, weight, size, and colour, and `br` forces a line break.
- Long paragraphs and unpainted boxes split across pages line by line instead of failing with `PageOverflow`. Decorated boxes, rows, and `break-inside: avoid` groups still move whole.
- Words wider than their column break at the last glyph that fits.
- FlateDecode compression of content streams and embedded font programs.
- `rgb()`, `rgba()`, `hsl()`, `hsla()`, 4- and 8-digit hex, and all 148 CSS named colours. Alpha other than 1 rejects with "opacity is not supported".
- `font-family` lists: the first registered family wins, and `Helvetica`, `Arial`, and generic families fall back to Helvetica.
- The `flex` shorthand (`flex: 1 1 0%`, `flex: none`). Only the grow factor is kept, and grow 0 requires a `width`.
- `!important` in stylesheets and inline style strings.
- `stylesheet` accepts a tagged template literal.
- An `edge-light` export condition for Vercel Edge.
- Runtime errors name the element and its ancestors, for example `on <span.total> in <div.row> > <main.invoice>`. Unsupported elements suggest a replacement.
- Component throws are wrapped as `<Name> threw while rendering` with `cause`. React context providers render their children, and `ref` and `key` on host elements are ignored.
- Input caps of 16 MiB per open container block and 16 MiB of painted content.
- CONTRIBUTING.md, a bug report issue form, a pull request checklist, and grouped Dependabot updates.

### Changed

- Embedded font programs are subset to the glyphs a document uses. Glyph ids are kept, unused outlines drop out, `cmap` and `OS/2` are omitted, and `/BaseFont` carries a deterministic six-letter subset tag.
- The TypeScript and protocol boundaries reject only control characters, lone surrogates, and page tokens outside a header or footer. Whether a printable character renders is decided per font at layout; Helvetica and Helvetica-Bold still reject characters outside WinAnsi.
- Flex-row cells stretch to the row height, so a shorter cell's background and borders now cover the whole row.
- The binary protocol adds header and metadata records and moves to version 6.
- The binary protocol adds `TableStart`/`TableEnd` records and moves to version 4, then adds an `Image` record, a per-paragraph URI table with underline, line-through, and link run flags, and `PdfRenderer.add_image`, and moves to version 5.
- Styled spans no longer break the paragraph. A span keeps its own line only when it has box properties, `width`, `flex`, or its own `text-align`. The binary protocol replaces the `Text` and `StyledText` records with one `Paragraph` record and moves to version 3.
- At-rules and rules with unsupported selectors are skipped instead of rejected. Declarations are validated only for rules that match a rendered element. `stylesheet()` still validates every rule with a supported selector.
- Descendant selectors backtrack, so `div > section p` matches through nested sections. Specificity compares (ids, classes, tags) as a tuple.
- An undefined `var()` without a fallback rejects instead of resolving to an empty string.
- Line height includes the font's line gap.
- Embedded fonts use their PostScript name for `/BaseFont`, OS/2 `sCapHeight` for `/CapHeight`, and a weight-class based `/StemV`.
- One entry point. The WASM loader is chosen through the `#wasm` imports map with `workerd`, `edge-light`, `browser`, and `default` conditions.
- `margin` and `padding` types accept one to four lengths, and `RenderOptions` fields accept an explicit `undefined`.
- The release workflow refuses tags whose version does not match `package.json` or whose commit is not on `main`.
- The WASM size gate is 400 KB raw and 200 KB gzipped. Rust is pinned to 1.94.0 through `rust-toolchain.toml`, and CI caches cargo artifacts.
- The packed-package E2E test validates the Node-rendered PDF with `qpdf --check` and prints a first-page raster hash from `mutool` when those tools are installed.
- Runtime support is Node 20.19 or newer. Development needs Node 22.12 or newer.

### Fixed

- Errors raised by the WASM renderer are `Error` objects, so they carry the element path and the decorated-box `PageOverflow` hint applies.
- Fonts whose `cmap` has only a format 12 (platform 3, encoding 10) subtable are accepted instead of rejected as unsupported.
- FontFile2 streams declare `/Length1`. Fonts with a positive descender are rejected, and degenerate hhea metrics fall back to OS/2 typo metrics when `USE_TYPO_METRICS` is set.
- Control characters and page-number tokens in body text are rejected at decode time.
- Node loading no longer uses a variable dynamic import, which removes webpack and Next.js critical-dependency warnings.
- Border shorthand accepts `rgb()` with spaces.
- Invalid embedded font errors name the requested font family.

## 0.2.2 - 2026-09-22

### Added

- Repeating `RenderOptions.footer` content with `PageNumber` and `TotalPages` tokens.
- Tested AWS Lambda Node.js packaging and a `workerd` export for Cloudflare Workers.
- Runnable Tailwind, StyleX, and styled-components static-CSS build examples.
- An isolated comparison benchmark for FlashPDF, PDFKit, React-pdf, and Takumi.

## 0.2.1 - 2026-09-17

### Changed

- JSX input now resolves through typed host and styled nodes before protocol compilation. This keeps React-compatible input validation at the public boundary and preserves the documented `render` and CSS APIs.

### Removed

- The unexported N-API backend and its native package artifact. FlashPDF now uses its WASM renderer in browsers, Node, and Bun.

## 0.2.0 - 2026-09-17

### Added

- Plain `span` and text siblings flow inline, per-edge border rules and `hr` are supported, and browser builds select a loader without Node filesystem imports.

### Fixed

- Vertical margin-only wrappers paginate, decorated overflow identifies its element, and CSS parsing avoids polynomial regular-expression paths.

## 0.1.1 - 2026-09-17

### Changed

- FlashPDF owns its JSX runtime and no longer requires React. Use `jsxImportSource: "@pettersen3008/flashpdf"` or the per-file pragma; React-created host-element trees remain supported.

## 0.1.0 - 2026-09-16

### Added

- Oxlint and Oxfmt checks for the TypeScript package.
- Host-provided embedded TrueType fonts through `RenderOptions.fonts`, selected by CSS/inline `fontFamily`, with regular and bold faces in browser/WASM, N-API, and Rust rendering.
- MIT license, package metadata, and an `exports` map with `types` conditions for every entry point.
- A packed-package E2E test covering Node, Bun, TypeScript, Vite, and Chromium consumers.
- `tests/golden/css-invoice.txt`, a content-stream golden that pins every draw position, colour, font, and page break of the CSS invoice.
- Heading defaults matching a browser UA sheet: `h1`-`h6` size in `em` and render bold unless a rule or inline style says otherwise.
- Standard React JSX, including pure function components, fragments, memo, forward refs, and class components.

### Changed

- Helvetica and Helvetica-Bold remain the fallback. Embedded-font text keeps the WinAnsi subset and rejects shaping, fallback lists, and faces beyond regular/bold.
- The WASM now ships inside the package at `wasm/`, so an installed `@pettersen3008/flashpdf` resolves its own module. Only the optimized build is published.
- Unsupported CSS names the reason it cannot be honoured, and at-rule and selector errors carry a source position.
- Rule order now runs across the whole `stylesheets` array, so a later sheet wins a tie at equal specificity.
- Text layout uses each font's ascent and descent, bold Helvetica widths, percentage flex-row columns, transparent paint, and ordered shorthand expansion.

### Fixed

- Pages using both the regular and bold font wrote `/Font` twice, which hid the regular font from viewers. Both now share one dictionary.
- `width` and `flex` outside a flex row were silently dropped. They now reject.
- CSS shorthand precedence, forwarded refs, React iterable and bigint children, and percentage rounding at exactly 100%.

### Removed

- The compiler export and its Lightning CSS dependency. Consumers now compile CSS in their own frontend build.
- The package-owned JSX runtime. React now creates the elements consumed by FlashPDF.
- The unreachable table, page-header, page-number, and total-pages protocol paths, along with the `hello`, `render_row`, and `render_table` WASM demos. The optimized WASM fell from 109,796 to 104,016 bytes.
