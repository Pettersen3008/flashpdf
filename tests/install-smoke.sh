#!/bin/sh
# Proves a published tarball renders from a clean install, with no workspace
# links and no repo-relative paths leaking into the package.
set -eu
cd "$(dirname "$0")/.."
root=$(pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

tarball=$(cd packages/flashpdf && npm pack --pack-destination "$work" --silent | tail -1)
cd "$work"
printf '{"name":"flashpdf-consumer","private":true,"type":"module"}\n' > package.json
npm install --silent --no-audit --no-fund "$work/$tarball"
cp "$root/packages/flashpdf/test/fixtures/Abel-Regular.ttf" font.ttf

cat > consumer.mjs <<'JS'
import { readFile } from 'node:fs/promises';
import { render, stylesheet } from '@flashpdf/core';
import { jsx, jsxs } from '@flashpdf/core/jsx-runtime';

const css = '.total { color: #0000ff; text-align: right }';
const document = jsxs('main', {
  style: { fontFamily: 'Invoice' },
  children: [jsx('h1', { children: 'Invoice' }), jsx('p', { className: 'total', children: '$100.00' })],
});
const font = new Uint8Array(await readFile('font.ttf'));
const pdf = await render(document, { pageFormat: 'A4', margin: 36, stylesheets: [stylesheet(css)], fonts: [{ family: 'Invoice', regular: font, bold: font }] });
if (!(pdf instanceof Uint8Array)) throw new Error('render must return a Uint8Array');
if ((Buffer.from(pdf).toString('latin1').match(/\/Subtype \/TrueType/g) ?? []).length !== 2) throw new Error('embedded regular and bold fonts missing');
process.stdout.write(Buffer.from(pdf).toString('base64'));
JS

node_pdf=$(node consumer.mjs)
case "$node_pdf" in JVBER*) ;; *) echo "node: not a PDF" >&2; exit 1 ;; esac
echo "node: ok"

if command -v bun >/dev/null 2>&1; then
  bun_pdf=$(bun consumer.mjs)
  [ "$node_pdf" = "$bun_pdf" ] || { echo "bun: output differs from node" >&2; exit 1; }
  echo "bun: ok"
else
  echo "bun not installed; Bun support is unverified on this target." >&2
fi

# Every declared export must resolve, and a TypeScript consumer must typecheck
# against the shipped .d.ts files, including the JSX runtime.
node -e '
const { createRequire } = require("node:module");
const resolve = createRequire(process.cwd() + "/consumer.mjs").resolve;
for (const entry of ["@flashpdf/core", "@flashpdf/core/jsx-runtime", "@flashpdf/core/jsx-dev-runtime", "@flashpdf/core/package.json"]) resolve(entry);
'
npm install --silent --no-audit --no-fund --save-dev typescript@5.9.3 @types/node@24
mkdir -p types
cp "$root/packages/flashpdf/examples/native-invoice.tsx" types/native-invoice.tsx
cat > types/tsconfig.json <<'JSON'
{"compilerOptions":{"strict":true,"outDir":"built","module":"esnext","target":"es2022","moduleResolution":"bundler","jsx":"react-jsx","jsxImportSource":"@flashpdf/core","types":["node"]},"include":["consumer.tsx","native-invoice.tsx"]}
JSON
cat > types/consumer.tsx <<'TS'
import { render, stylesheet, Fragment, type EmbeddedFont, type RenderOptions } from '@flashpdf/core';

const options: RenderOptions = { pageFormat: 'A4', margin: 36 };
const font: EmbeddedFont = { family: 'Invoice', regular: new Uint8Array(), bold: new Uint8Array() };
export async function invoice(total: string): Promise<Uint8Array> {
  return render(
    <main>
      <h1>Invoice</h1>
      <><p className="total">{total}</p></>
    </main>,
    { ...options, stylesheets: [stylesheet('.total { text-align: right }')], fonts: [font] },
  );
}
export const fragment = Fragment;
TS
npx tsc -p types/tsconfig.json

# The starter template ships in the README, so it has to render against the
# published package, not just typecheck against it.
node --input-type=module -e '
import { nativeInvoice } from "./types/built/native-invoice.js";
const pdf = await nativeInvoice({
  number: "2026-0042", issued: "2026-09-01", due: "2026-09-30", currency: "EUR", taxRate: 0.25,
  seller: { name: "Acme Supply Co.", address: "1 Harbour Road, Oslo" },
  customer: { name: "Beta Industries", address: "9 Market Street, Bergen" },
  items: Array.from({ length: 60 }, (_, index) => ({ description: `Line item ${index + 1}`, quantity: index + 1, unitPrice: 12.5 })),
});
if (Buffer.from(pdf.subarray(0, 5)).toString() !== "%PDF-") throw new Error("starter example did not render a PDF");
console.log(`starter example: ok (${pdf.length} bytes)`);
'
echo "exports and types: ok"

# A Vite consumer proves the browser entry resolves its own WASM asset and pulls
# in no Node builtins or build-only CSS compiler.
npm install --silent --no-audit --no-fund --save-dev vite@7
mkdir -p src
cat > index.html <<'HTML'
<!doctype html><html><body><pre id="out">rendering</pre><script type="module" src="/src/main.js"></script></body></html>
HTML
cat > src/invoice.css <<'CSS'
.total { color: #0000ff; text-align: right }
CSS
cat > src/main.js <<'JS'
import { render } from '@flashpdf/core';
import { jsx, jsxs } from '@flashpdf/core/jsx-runtime';
import css from './invoice.css?inline';

const document_ = jsxs('main', {
  children: [jsx('h1', { children: 'Invoice' }), jsx('p', { className: 'total', children: '$100.00' })],
});
const pdf = await render(document_, { pageFormat: 'A4', margin: 36, stylesheets: [css] });
document.querySelector('#out').textContent = `${new TextDecoder().decode(pdf.subarray(0, 5))} ${pdf.length}`;
JS

npx vite build --logLevel warn
bundle=$(cat dist/assets/*.js)
ls dist/assets/*.wasm >/dev/null || { echo "vite: no WASM asset emitted" >&2; exit 1; }
case "$bundle" in
  *lightningcss*) echo "vite: bundle reaches Lightning CSS" >&2; exit 1 ;;
  *externalized\ for\ browser*) echo "vite: bundle externalizes a Node builtin" >&2; exit 1 ;;
esac
echo "vite: ok"
