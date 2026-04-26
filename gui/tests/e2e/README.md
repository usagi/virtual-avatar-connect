# `gui/tests/e2e/` — Phase ν Playwright E2E specs

Design doc: [`docs/roadmap/phase-nu-gui-e2e-playwright.md`](../../../docs/roadmap/phase-nu-gui-e2e-playwright.md)

## Layout

```text
gui/tests/e2e/
  README.md                  ← this file
  smoke.spec.ts              ← ν-1 scaffolding smoke (verifies webServer + auth path)
  fixtures/
    flowgraph/               ← fixture *.flowgraph.toml loaded by conf.fixture.e2e.toml
      sample.flowgraph.toml
```

ν-2 will add the remaining 4 specs here:

- `flowgraph-canvas-basic.spec.ts`
- `dictionary-editor-409-merge.spec.ts`
- `live-quick-add-learn-undo.spec.ts`
- `channels-ws-live-update.spec.ts`

GUI redesign specs are added incrementally and should stay narrow:

- `now-dashboard.spec.ts`
- `main-navigation.spec.ts`

## Running locally

From the repository root:

```bash
cd gui
npm install                        # once
npm run test:e2e:install           # downloads chromium (~150MB), once
npm run test:e2e                   # headless
npm run test:e2e:ui                # Playwright UI (time-travel)
```

The `webServer` entry in `playwright.config.ts` boots VAC itself against
`conf.fixture.e2e.toml` on port `57098`. Make sure that port is free
before running (`netstat -ano | findstr :57098` on Windows,
`ss -tlnp | grep 57098` on Linux).

The fixture ships a fixed Bearer token (`e2e-fixture-token`) — specs
attach it via `?token=...` which the GUI's `auth.ts` persists to
localStorage. There is intentionally **no .env support**: E2E must be
reproducible across machines.

## Selector policy

Prefer `getByRole` / `getByLabel` / `getByText`. Only reach for
`data-testid` when the role/name approach is genuinely unavailable
(e.g. inside `@xyflow/svelte` canvas internals). If you add one, name
it `vac-<area>-<role>`, e.g. `vac-flowgraph-node-<id>`.
