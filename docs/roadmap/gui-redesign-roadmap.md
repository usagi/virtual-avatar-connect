# VAC GUI Redesign Roadmap

> Status: implementation branch kickoff. This document defines the GUI redesign direction for v2: VAC GUI moves from a feature-tab control panel to a resident runtime cockpit plus Flowgraph Studio.

---

## 0. Goal

VAC v2 is an always-on dataflow application. The GUI should therefore answer the user's first question immediately:

```text
What is VAC doing right now, what needs attention, and what can I safely change?
```

The current GUI exposes useful surfaces, but its top-level structure still reflects a feature bucket model:

- Live
- Setup
- Flowgraph
- Logs
- Tools

The v2 GUI will use an operational model instead:

- Now
- Modes
- Flowgraph Studio
- Resources
- Observability
- Settings

This is a breaking information-architecture change. Existing lower-level panels should be reused where they still fit, but the top-level shell should be rebuilt around runtime state.

---

## 1. Principles

### 1.1 Now-first

The first screen is `Now`, not setup and not the editor. A resident app must show current state before configuration.

`Now` aggregates:

- WebSocket / Control API connection state
- current runtime mode
- AI persona state
- Flowgraph load health
- Managed App state
- recent important events

### 1.2 Modes are first-class

Runtime Mode is a primary user workflow. The GUI must make mode switching visible and reversible enough to trust.

Initial UI can show planned mode concepts before the backend lands, but the final UI must support:

- current mode indicator
- transition dry-run preview
- transition request
- transition progress / result
- per-mode Flowgraph / Managed App summary

### 1.3 Flowgraph Studio is an IDE surface

The Flowgraph editor should feel like a compact IDE:

- file tree
- canvas
- command palette
- searchable node insert
- inspector
- problems panel
- undo / redo
- multi-select
- grouping / alignment

The initial branch does not implement all of this. It changes the top-level IA first so later PRs have a stable target.

### 1.4 Resources are external runtime dependencies

Managed Apps, OAuth, OBS / Warudo / TTS, and future integrations belong under `Resources`. They are not general settings; they are runtime dependencies VAC can start, stop, authorize, or inspect.

### 1.5 Observability is separate from editing

Logs, event timeline, Flowgraph diagnostics, and runtime history are operational observation surfaces. They should not be hidden behind editor panels.

---

## 2. Target Navigation

### 2.1 Now

Operational dashboard. Shows current state and recent runtime activity.

### 2.2 Modes

Runtime mode management. This is wired to the Runtime Mode roadmap when the backend lands.

### 2.3 Flowgraph Studio

Flowgraph authoring and debugging. Existing `FlowgraphTab` is renamed conceptually first; deep editor refactors happen in later PRs.

### 2.4 Resources

External dependencies and credentials:

- Managed Apps
- OAuth
- run_with registry
- OBS / avatar / TTS integrations

Existing `SetupTab` content can move here in stages.

### 2.5 Observability

Runtime observation:

- logs
- event stream
- diagnostics
- transition results

Existing `LogsTab` is the first implementation.

### 2.6 Settings

Configuration and dangerous operations:

- profile management
- restart / shutdown
- reload
- developer tools

Existing `ToolsTab` can remain here until split further.

---

## 3. Implementation Plan

### GR-1 Shell IA

- Replace top-level tab list with `Now / Modes / Flowgraph Studio / Resources / Observability / Settings`.
- Use a responsive navigation shell: horizontal on narrow screens, left rail on desktop.
- Keep old hash redirects:
  - `#live` -> `#now`
  - `#setup` -> `#resources`
  - `#logs` -> `#observability`
  - `#tools` -> `#settings`
  - `#pipeline` -> `#flowgraph`
- Add `NowTab`.
- Add `ModesTab` placeholder that is explicit about unavailable backend state without inventing fake control.

### GR-2 Now Dashboard

- Aggregate existing endpoints only:
  - `/snapshot`
  - `/managed_apps`
  - `/flowgraph/tree`
  - `/flowgraph/diagnostics`
  - Control WS event buffer
- Do not add backend API in this PR.

### GR-3 Resource Re-home

- Move OAuth, reload, pause, profile, and run_with panels into the new IA.
- Split operational resources from restart / destructive settings.
- Initial split:
  - `Resources`: run_with / Managed App registry, Pause / Resume, OAuth, future OBS / avatar / TTS connectors.
  - `Settings`: profile management, reload, Control API connection info, developer utilities.

### GR-4 Flowgraph Studio Layout

- Replace fixed 3-pane layout with a resizable editor shell.
- Rename property editor role to Inspector.
- Move diagnostics into a Problems panel.
- Add command palette and node search as primary node insertion path.
- First slice: add a Studio heading, file / node / edge summary, named workspace regions, Inspector label, and Problems panel while keeping the existing editor behavior intact.

### GR-5 Editor Operations

- Undo / redo stack.
- Multi-select as a real data model.
- Group / align / duplicate / delete commands.
- E2E coverage for keyboard and mouse editing.

### GR-6 Runtime Modes UI

- Wire to Runtime Mode backend.
- Add dry-run transition plan preview.
- Display Flowgraph activation and Managed App desired-state changes.
- Before backend support, keep the Modes surface read-only: planned mode cards, static transition preview, and disabled transition action only.

### GR-7 Observability

- Event timeline with filters.
- Flowgraph execution history.
- Runtime mode transition history.
- Managed App event history.
- First slice: expose Observability as an explicit evidence surface around the existing runtime snapshot and event stream before adding new backend history APIs.

---

## 4. First Branch Scope

The first GUI branch implements only GR-1 and the first slice of GR-2:

- new top-level navigation ids and labels
- `NowTab` based on existing APIs
- `ModesTab` placeholder
- smoke test update

No Flowgraph editor internals are changed in this slice.
