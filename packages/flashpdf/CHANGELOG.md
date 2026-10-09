# Changelog

## Unreleased

### Changed

- The engine compiles for speed instead of size (`opt-level = 3`, `wasm-opt -O3`). Layout and PDF writing in WASM run about 32% faster; the WASM grows from 241 KB to 295 KB raw (114 KB to 125 KB gzip). Output bytes are unchanged.
### Fixed

- With a header, a line or image taller than the page rejects with `PageOverflow` instead of looping until the renderer runs out of memory.
- Blocks that move whole, such as decorated boxes, rows, and table rows, reject when they do not fit between the header and footer, instead of painting over the footer and into the bottom margin.
- Headers, footers, and repeated table headers count toward the 16 MiB painted-content cap as they paint, so a long footer on thousands of pages rejects with `DocumentTooLarge` instead of growing to gigabytes.
- A line that mixes font sizes or line heights is as tall as its highest ascent plus its lowest depth below the baseline, so it no longer overlaps the next line.
### Fixed

- A PNG with transparency stops inflating at the size its dimensions allow, so a small compressed image can no longer exhaust memory.
- Embedded fonts use one `cmap` subtable: format 12 before format 4, then Windows before Unicode platform. A font with many records or overlapping format 4 segments no longer takes minutes to load, and a format 12 group that starts inside the previous one keeps its remaining characters.
- Registered fonts are capped at 64 MiB per render, checked in JavaScript before the bytes are copied into WASM memory.
- Page sides over 14,400 pt reject, the largest size PDF readers accept.
### Fixed

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

### Changed

- `font-weight` rejects values other than `normal`, `bold`, and 1 to 1000, such as `bolder` or a typo. They rendered regular before.
- `<hr>` and `<br>` reject children, `<br>` rejects box and layout styles, and `width` on an `<hr>` outside a flex row rejects. All were dropped before.
- `<td>` and `<th>` outside a table row reject. They rendered as a `<div>` before.
- `stylesheet()` validates declaration values, not only property names, so `.a { color: notacolor }` rejects up front, naming the selector and position. Values that use `var()` are still checked at `render`.
- U+001E and U+001F reject in user text. They are the page number and total pages tokens, so they threw in body text and became page numbers in a footer. `PageNumber` and `TotalPages` still emit them.
- A `width` or image `height` too large for a 32-bit float rejects as an invalid width instead of reaching the engine.
### Added

- `RenderOptions.pdfa: '3b'` writes PDF/A-3b: XMP metadata that repeats `metadata`, an sRGB output intent, and a document ID hashed from the content. In this mode, text that would use the unembedded Helvetica and CMYK JPEGs reject.
- `RenderOptions.attachments` embeds files with an `AFRelationship`, listed in the catalog's `/AF` array and the EmbeddedFiles name tree. Names, MIME types, and relationships are validated, with a 64 MiB cap per render.
- `RenderOptions.facturx: { xml, profile }` embeds Factur-X or ZUGFeRD XML as `factur-x.xml` (`xrechnung.xml` for `XRECHNUNG`) with the Factur-X XMP extension schema, and implies `pdfa: '3b'`. FlashPDF does not validate the XML content.

### Changed

- Link annotations carry the print flag, which PDF/A requires.

## 0.4.0 - 2026-10-02

### Added

- `line-height`: `normal`, a unitless factor, a percent, or a length. A factor inherits as a factor and scales with each descendant's font size; a percent or `em` resolves against the element's own size and inherits as points. The extra height splits evenly above and below the text, as CSS half-leading does.
- `gap` on a flex row. The gap is a fixed-width empty column between items, so `flex` grow shares the remaining width.
- `tfoot` rows repeat after the last body row of every page a table spans, and their height is reserved on every page. Previously they painted once at the end.

### Changed

- The binary protocol is version 7: `TableStart` carries a footer row count. The JavaScript and WASM halves ship together, so this affects no public API.

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
