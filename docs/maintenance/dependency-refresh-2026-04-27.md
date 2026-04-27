# Dependency Refresh - 2026-04-27

This PR applies the low-risk dependency update slice identified by the health baseline.

## Updated

GUI lockfile updates within the existing `package.json` semver ranges:

- `@rolldown/binding-win32-x64-msvc` and the Rolldown optional binding set to `1.0.0-rc.17`
- `@tailwindcss/vite` / `tailwindcss` to `4.2.4`
- `eslint-plugin-svelte` to `3.17.1`
- `svelte` to `5.55.5`
- `typescript-eslint` packages to `8.59.0`
- `vite` to `8.0.10`

No Rust lockfile updates were available under the current semver constraints.

## Intentionally Not Updated

- `@types/node` remains on `24.12.2`; `25.6.0` is a major update and should be evaluated separately.
- Rust crates outside current constraints remain unchanged:
  - `async-openai`
  - `sha2`
  - `tokio-tungstenite`
  - `toml_edit`
  - `zip`

## Validation

- `cargo update --dry-run --verbose`
- `cargo check`
- `npm audit --audit-level=moderate`
- `npm outdated`
- `npm run check`
- `npm run lint`
- `npm run build`
- `npm run test:e2e -- control-panel-smoke.spec.ts main-navigation.spec.ts modes-planning.spec.ts`
