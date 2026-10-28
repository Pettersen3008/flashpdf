#!/bin/sh
# Builds the WASM the package ships and gates its size. verify.sh and the release job both run it.
set -eu
cd "$(dirname "$0")"

wasm=packages/flashpdf/wasm
wasm-pack build crates/flashpdf-wasm --release --target web --out-dir ../../$wasm --locked
# wasm-opt writes back over the glue's default path, so the package ships one
# WASM file and a bundler resolves it without a fallback URL.
mv $wasm/flashpdf_wasm_bg.wasm target/flashpdf_wasm_bg.raw.wasm
# --low-memory-unused lets wasm-opt fold constant offsets into loads and stores (-16 KB). It
# assumes addresses below 1 KiB stay unused: Rust puts its 1 MiB stack there, growing down,
# so only a stack within 1 KiB of overflowing would reach them.
pnpm exec wasm-opt -O3 --low-memory-unused \
  --enable-bulk-memory --enable-nontrapping-float-to-int \
  target/flashpdf_wasm_bg.raw.wasm -o $wasm/flashpdf_wasm_bg.wasm

raw=$(wc -c < $wasm/flashpdf_wasm_bg.wasm)
gzip -9 -n -c $wasm/flashpdf_wasm_bg.wasm > target/flashpdf_wasm_bg.wasm.gz
compressed=$(wc -c < target/flashpdf_wasm_bg.wasm.gz)
wc -c target/flashpdf_wasm_bg.raw.wasm $wasm/flashpdf_wasm_bg.wasm \
  target/flashpdf_wasm_bg.wasm.gz $wasm/flashpdf_wasm.js
! command -v brotli >/dev/null || echo "brotli: $(brotli -c $wasm/flashpdf_wasm_bg.wasm | wc -c)"
if [ "$raw" -gt 2100000 ] || [ "$compressed" -gt 850000 ]; then
  cargo tree --locked -p flashpdf-wasm --target wasm32-unknown-unknown -e normal
  echo "Size gate failed; stop before adding features." >&2
  exit 1
fi
