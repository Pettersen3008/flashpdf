# Contributing

[AGENTS.md](./AGENTS.md) holds the repository map, requirements, and constraints. Read it first.

## Pull requests

1. Fork the repository and create a branch from `main`.
2. Install pnpm 12.4.2 with `npm install -g pnpm@12.4.2`, because `packageManager` pins it and older pnpm cannot switch to it. Make the change, then run `sh verify.sh`. It is the source of truth and CI runs the same script.
3. Add an entry under `## Unreleased` in `packages/flashpdf/CHANGELOG.md` for anything a user can notice.
4. Update README.md, CSS.md, or AGENTS.md when the behaviour they describe changes.
5. Open the pull request and complete the checklist in the template.

Report security problems through the process in [SECURITY.md](./SECURITY.md), not in a public issue.

This project follows the [Contributor Covenant](./CODE_OF_CONDUCT.md), version 2.1.
