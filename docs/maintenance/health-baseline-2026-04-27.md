# Health Baseline - 2026-04-27

This document captures the v2 maintenance baseline before dependency updates and broader structural cleanup.

## Scope

- Establish the current check status.
- Fix low-risk lint/check issues that do not change behavior.
- Record dependency update candidates without changing lockfiles.
- Keep dependency upgrades and larger refactors for follow-up PRs.

## Check Status

| Command | Result | Notes |
| --- | --- | --- |
| `cargo check` | Passes with 19 warnings | Two unused imports in `flowgraph/quantity/parser.rs` were removed in this PR. Remaining warnings are dead code or intentionally unused API/test helpers. |
| `cargo fmt --check` | Fails | Broad existing Windows newline-style mismatch plus rustfmt diffs across many files. Treat as a dedicated line-ending/format policy PR, not as incidental churn. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Fails | Current baseline reports many existing warnings, including dead code, test lint nits, doc indentation, and some complexity/style lints. Do not enable as a hard gate yet. |
| `npm run check` | Passes | Svelte and TypeScript checks are clean. |
| `npm run lint` | Passes | Fixed keyed each blocks, a DOM listener type issue, one useless mustache interpolation, Svelte reactive collection lint issues, and stale eslint-disable comments. |
| `npm audit --audit-level=moderate` | Passes | 0 vulnerabilities. |

## Rust Warning Baseline

`cargo check` still reports 19 warnings after this PR:

- OpenAI Responses client/testing helpers that are not currently used by production code.
- Streaming response fields that are preserved for protocol completeness but not yet read by consumers.
- A few dormant helper functions or fields in motion, VoiceVox, Twitch OAuth, shutdown, and GUI path handling.

Recommendation: do not delete these in bulk. Either wire them into planned features, move test-only helpers under `#[cfg(test)]`, or add narrow `#[allow(dead_code)]` with rationale when the API is intentionally reserved.

## Clippy Baseline

`cargo clippy --all-targets --all-features -- -D warnings` is not ready as a required gate. The highest-signal groups are:

- Safe mechanical fixes: inconsistent digit grouping, `div_ceil`, `as_deref`, `first()`, bool asserts, `is_empty`, needless borrows, redundant closures.
- Intentional design exceptions: long argument lists, complex types, methods named `to_*` that consume self.
- Test-only cleanup: field reassignment after default, approximate constants, items after test modules.
- Documentation formatting: lazy continuation indentation in rustdoc comments.
- Dead code policy: response protocol fields and reserved helpers.

Recommendation: introduce clippy gradually. Start with mechanical test/lint fixes, then decide which lints are policy and which are allowed.

## Dependency Baseline

### Rust

`cargo update --dry-run --verbose` reports no lockfile updates under current semver constraints. It also reports these newer versions outside current constraints:

| Crate | Current | Available | Notes |
| --- | --- | --- | --- |
| `async-openai` | 0.34.0 | 0.36.1 | Review API compatibility separately. |
| `generic-array` | 0.14.7 | 0.14.9 | Transitive. |
| `sha2` | 0.10.9 | 0.11.0 | Major/minor constraint change. |
| `tokio-tungstenite` | 0.26.2 | 0.29.0 | WebSocket stack, test carefully. |
| `toml_edit` | 0.23.10 | 0.25.11 | TOML mutation paths need regression tests. |
| `ureq` | 2.12.1 | 3.3.0 | Transitive. |
| `zip` | 2.4.2 | 8.6.0 | Major jump; isolate if updated. |

### GUI

`npm outdated` reports patch/minor update candidates:

| Package | Current | Wanted | Latest |
| --- | --- | --- | --- |
| `@rolldown/binding-win32-x64-msvc` | 1.0.0-rc.16 | 1.0.0-rc.17 | 1.0.0-rc.17 |
| `@tailwindcss/vite` | 4.2.2 | 4.2.4 | 4.2.4 |
| `@types/node` | 24.12.2 | 24.12.2 | 25.6.0 |
| `eslint-plugin-svelte` | 3.17.0 | 3.17.1 | 3.17.1 |
| `svelte` | 5.55.4 | 5.55.5 | 5.55.5 |
| `tailwindcss` | 4.2.2 | 4.2.4 | 4.2.4 |
| `typescript-eslint` | 8.58.2 | 8.59.0 | 8.59.0 |
| `vite` | 8.0.8 | 8.0.10 | 8.0.10 |

Recommendation: update GUI patch/minor dependencies first in the dependency-update PR, keep `@types/node` major separate unless needed.

## Follow-up PR Order

1. Dependency update PR:
   - GUI patch/minor updates first.
   - Rust updates only where semver-compatible or clearly scoped.
   - Run `cargo check`, `npm run check`, `npm run lint`, `npm run build`, and focused Playwright smoke tests.

2. Structure cleanup PR:
   - Consolidate GUI API/type/store boundaries.
   - Review `web_interface/control/*` module naming after the v2 split.
   - Remove or justify dead code once the dependency update branch is stable.

3. Format policy PR:
   - Decide repository line-ending policy for Rust sources.
   - Apply rustfmt only when the line-ending churn is intentional and isolated.
