# Security Policy

## Reporting a vulnerability

Use GitHub's private vulnerability reporting for this repository from the
[Security](https://github.com/Pettersen3008/flashpdf/security) tab. Do not
open a public issue for an unpatched vulnerability.

Include the affected version or commit, the impact, reproduction steps, and
any useful logs or proof of concept. Remove secrets and personal data before
submitting.

## Support window

Security fixes target the latest `@pettersen3008/flashpdf` release on npm and
the latest `main` commit. Older releases receive no fixes.

## Untrusted input

FlashPDF treats CSS strings, JSX props, and font bytes as untrusted. The
TypeScript layer validates them before encoding the layout protocol, and the
Rust core validates the protocol and TrueType tables again. The WASM module
aborts on panic instead of continuing with corrupt state. Mutation tests cover
the protocol decoder and the TrueType parser.
