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
npx --yes --package=binaryen@123.0.0 wasm-opt -Oz \
  --enable-bulk-memory --enable-nontrapping-float-to-int \
  target/flashpdf_wasm_bg.raw.wasm -o $wasm/flashpdf_wasm_bg.wasm

raw=$(wc -c < $wasm/flashpdf_wasm_bg.wasm)
gzip -9 -n -c $wasm/flashpdf_wasm_bg.wasm > target/flashpdf_wasm_bg.wasm.gz
compressed=$(wc -c < target/flashpdf_wasm_bg.wasm.gz)
wc -c target/flashpdf_wasm_bg.raw.wasm $wasm/flashpdf_wasm_bg.wasm \
  target/flashpdf_wasm_bg.wasm.gz $wasm/flashpdf_wasm.js
if [ "$raw" -gt 120000 ] || [ "$compressed" -ge 100000 ]; then
  cargo tree --locked -p flashpdf-wasm --target wasm32-unknown-unknown -e normal
  echo "Size gate failed; stop before adding features." >&2
  exit 1
fi
node --test tests/wasm-smoke.mjs

cargo build --locked --release -p flashpdf-napi
mkdir -p dist/napi
case "$(uname -s)" in
  Darwin) cp target/release/libflashpdf_napi.dylib dist/napi/flashpdf.node ;;
  *) cp target/release/libflashpdf_napi.so dist/napi/flashpdf.node ;;
esac

pnpm typecheck
pnpm test

if command -v bun >/dev/null 2>&1; then
  bun test packages/flashpdf/test/native.test.mjs
else
  echo "bun not installed; Bun support is unverified on this target." >&2
fi

# Packs the tarball and renders from a clean install in Node, Bun and a Vite
# browser build. Needs network access for npm install.
sh tests/install-smoke.sh
