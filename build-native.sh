#!/bin/sh
# Builds the N-API addon into the per-platform package the loader requires,
# packages/flashpdf/npm/<platform>, for the host or for TARGET (a Rust triple).
set -eu
cd "$(dirname "$0")"

target=${TARGET:-$(rustc -vV | sed -n 's/^host: //p')}
case "$target" in
  aarch64-apple-darwin) platform=darwin-arm64 os=darwin cpu=arm64 lib=libflashpdf_napi.dylib ;;
  x86_64-apple-darwin) platform=darwin-x64 os=darwin cpu=x64 lib=libflashpdf_napi.dylib ;;
  x86_64-unknown-linux-gnu) platform=linux-x64-gnu os=linux cpu=x64 lib=libflashpdf_napi.so ;;
  aarch64-unknown-linux-gnu) platform=linux-arm64-gnu os=linux cpu=arm64 lib=libflashpdf_napi.so ;;
  *) echo "no native package for $target; it renders through WASM" >&2; exit 1 ;;
esac

cargo build --locked --release -p flashpdf-napi --target "$target"
out=packages/flashpdf/npm/$platform
mkdir -p "$out"
cp "target/$target/release/$lib" "$out/flashpdf.node"
version=$(sed -n 's/^."version": "\(.*\)",$/\1/p' packages/flashpdf/package.json)
case "$platform" in *-gnu) libc='"libc": ["glibc"], ' ;; *) libc= ;; esac
cat > "$out/package.json" <<JSON
{"name": "@pettersen3008/flashpdf-$platform", "version": "$version", "license": "MIT", "repository": {"type": "git", "url": "https://github.com/Pettersen3008/flashpdf"}, "os": ["$os"], "cpu": ["$cpu"], ${libc}"main": "flashpdf.node", "files": ["flashpdf.node"], "publishConfig": {"@pettersen3008:registry": "https://npm.pkg.github.com", "access": "public"}}
JSON
wc -c "$out/flashpdf.node"
