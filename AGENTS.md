# Contributing to FlashPDF

This file gives coding agents and contributors the public rules for working in
the repository. Keep changes small and preserve the existing API unless the
task explicitly changes it.

## Repository map

- `crates/flashpdf-core` contains the Rust layout and PDF engine.
- `crates/flashpdf-wasm` exposes the engine to browsers and JavaScript.
- `crates/flashpdf-napi` provides the native test backend.
- `packages/flashpdf/src` contains the TypeScript JSX, CSS, and binary adapters.
- `packages/flashpdf/test` contains package and backend tests.
- `tests` contains clean-install, WASM, and golden-output checks.
- `CSS.md` documents the supported CSS subset and intentional limits.

## Development

Requirements: Node 20+, pnpm, Rust, and `wasm-pack`. Bun 1.x is optional.

```bash
pnpm install --frozen-lockfile
sh verify.sh
```

`sh verify.sh` is the source of truth. It runs formatting, linting, Rust and
TypeScript tests, WASM size gates, backend parity checks, and the clean-install
package smoke test.

## Constraints

- Keep optimized WASM at or below 120 KB raw and below 100 KB gzipped.
- Keep browser and native output byte-for-byte identical for the same input.
- Validate untrusted input at the TypeScript and Rust boundaries.
- Do not commit `target/`, `dist/`, `node_modules/`, or generated package output.
- Keep repository documentation public. Do not add chat transcripts, handoff
  notes, private plans, local paths, or generated verification logs.
