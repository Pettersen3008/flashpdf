# Changelog

## Unreleased

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
