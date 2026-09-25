#!/bin/sh
set -eu
cd "$(dirname "$0")"

cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
pnpm fmt:check
pnpm lint
wasm=packages/flashpdf/wasm
wasm-pack build crates/flashpdf-wasm --release --target web --out-dir ../../$wasm --locked
# wasm-opt writes back over the glue's default path, so the package ships one
# WASM file and a bundler resolves it without a fallback URL.
mv $wasm/flashpdf_wasm_bg.wasm target/flashpdf_wasm_bg.raw.wasm
pnpm exec wasm-opt -Oz \
  --enable-bulk-memory --enable-nontrapping-float-to-int \
  target/flashpdf_wasm_bg.raw.wasm -o $wasm/flashpdf_wasm_bg.wasm

raw=$(wc -c < $wasm/flashpdf_wasm_bg.wasm)
gzip -9 -n -c $wasm/flashpdf_wasm_bg.wasm > target/flashpdf_wasm_bg.wasm.gz
compressed=$(wc -c < target/flashpdf_wasm_bg.wasm.gz)
wc -c target/flashpdf_wasm_bg.raw.wasm $wasm/flashpdf_wasm_bg.wasm \
  target/flashpdf_wasm_bg.wasm.gz $wasm/flashpdf_wasm.js
if [ "$raw" -gt 400000 ] || [ "$compressed" -ge 200000 ]; then
  cargo tree --locked -p flashpdf-wasm --target wasm32-unknown-unknown -e normal
  echo "Size gate failed; stop before adding features." >&2
  exit 1
fi
pnpm typecheck
pnpm --filter @pettersen3008/flashpdf examples:css
pnpm exec vitest run
node tests/e2e.mjs
