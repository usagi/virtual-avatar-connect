# Structure Cleanup - 2026-04-27

This PR is the third maintenance slice after the health baseline and dependency refresh.

## Scope

- Keep behavior unchanged.
- Move presentation-only Runtime Mode helpers out of the Svelte tab component.
- Move ControlEvent kind classification from the web history endpoint to the event type itself.
- Leave larger module reshapes for later branches.

## Changes

### GUI

`gui/src/lib/tabs/ModesTab.svelte` now delegates pure display logic to `gui/src/lib/control/runtimeModes.ts`:

- planned Runtime Mode card definitions
- configured/custom mode display row construction
- managed app desired-state row construction
- Control API error formatting
- list formatting

The tab component remains responsible for UI state, polling, API calls, and rendering.

### Control API

`ControlEvent::kind()` now lives in `src/control_events.rs`. The event history endpoint uses that method for `kind` filtering instead of carrying a separate local match in `src/web_interface/control/events.rs`.

This keeps event classification next to the event enum, so future event variants have one obvious place to update.

## Deferred

- Splitting `gui/src/lib/types.ts` into domain type files. The file is large, but moving exported API types now would cause broad import churn.
- Reshaping `src/web_interface/control/modes.rs`. It is a candidate for a later `types.rs` / `handlers.rs` split, but doing that together with active Runtime Mode work would create unnecessary conflict risk.
- Repository-wide rustfmt or line-ending changes. That remains a dedicated format policy task.

## Validation

- `cargo check`
- `npm run check`
- `npm run lint`
- `npm run build`
- `npm run test:e2e -- modes-planning.spec.ts observability-layout.spec.ts`
