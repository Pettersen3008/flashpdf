#!/bin/sh
set -eu
cd "$(dirname "$0")"

cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
pnpm fmt:check
pnpm lint
sh build.sh
sh build-native.sh
pnpm typecheck
pnpm --filter @pettersen3008/flashpdf examples:css
pnpm exec vitest run
node tests/e2e.mjs
