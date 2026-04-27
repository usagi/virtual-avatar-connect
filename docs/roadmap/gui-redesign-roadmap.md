# VAC GUI Redesign Roadmap

> Status: implementation branch active. The cockpit shell, Runtime Mode surface, Flowgraph Studio operations, Resource / Settings split, Observability history surfaces, transition progress UX, and Playwright coverage are now implemented on the GUI branch. Remaining work is refinement: visual polish, deeper domain-specific debugger views, and future backend integrations that are not yet specified.

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

### GR-1 Shell IA - implemented

- [x] Replace top-level tab list with `Now / Modes / Flowgraph Studio / Resources / Observability / Settings`.
- [x] Use a responsive navigation shell: horizontal on narrow screens, left rail on desktop.
- [x] Keep old hash redirects:
  - `#live` -> `#now`
  - `#setup` -> `#resources`
  - `#logs` -> `#observability`
  - `#tools` -> `#settings`
  - `#pipeline` -> `#flowgraph`
- [x] Add `NowTab`.
- [x] Add `ModesTab`.

### GR-2 Now Dashboard - implemented first slice

- [x] Aggregate existing endpoints only:
  - `/snapshot`
  - `/modes/current`
  - `/managed_apps`
  - `/flowgraph/tree`
  - `/flowgraph/diagnostics`
  - Control WS event buffer
- [x] Do not add backend API in this PR.

### GR-3 Resource Re-home - implemented first slice

- [x] Move OAuth, reload, pause, profile, and run_with panels into the new IA.
- [x] Split operational resources from restart / destructive settings.
- [x] Initial split:
  - `Resources`: run_with / Managed App registry, Pause / Resume, OAuth, future OBS / avatar / TTS connectors.
  - `Settings`: profile management, reload, Control API connection info, developer utilities.

### GR-4 Flowgraph Studio Layout - implemented first and second slices

- [x] Add a Studio heading, file / node / edge summary, named workspace regions, Inspector label, and Problems panel while keeping the existing editor behavior intact.
- [x] Add a command palette entry point that groups existing Flowgraph operations before deeper editor command modeling lands.
- [x] Replace the current framed editor shell with user-resizable file / inspector / problems panes.
- [x] Add searchable node insert backed by command modeling.

### GR-5 Editor Operations

- [x] Undo / redo stack for draft node, edge, property, and position edits.
- [x] Multi-select as a real data model for Flowgraph Studio commands.
- [x] Group-layout / align / distribute / duplicate / delete commands.
- [x] E2E coverage for command palette operation discovery.

### GR-6 Runtime Modes UI - implemented backend wiring

- [x] Wire to Runtime Mode backend.
- [x] Add dry-run transition plan preview via `/modes/plan`.
- [x] Request transitions via `/modes/transit`.
- [x] Display Flowgraph activation and Managed App desired-state changes from the backend plan.
- [x] Keep planned mode previews visible, but enable transition only for modes present in `conf [modes]`.
- [x] Add transition-progress UX backed by `/modes/transition` server-side progress state.

### GR-7 Observability - implemented first slice

- [x] Expose Observability as an explicit evidence surface around the existing runtime snapshot and event stream.
- [x] Event timeline with filters for current ControlEvent kinds, including Runtime Mode events.
- [x] Server-side ControlEvent history ring buffer exposed by `/events/history`.
- [x] Flowgraph reload / restart recommendation history.
- [x] Runtime mode transition history.
- [x] Managed App event history.

---

## 4. Current Branch Scope

The current GUI branch now includes these completed slices:

- GR-1 shell IA and legacy hash fallback
- GR-2 Now dashboard using existing Control API endpoints
- GR-3 Resources / Settings split for operational vs durable controls
- GR-4 Flowgraph Studio framing, resizable panes, and searchable command palette / node insert
- GR-5 editor operation commands: undo / redo, multi-select, group-layout, align, distribute, duplicate, delete
- GR-6 Runtime Mode backend wiring, dry-run preview, transit action, and desired-state display
- GR-6 Runtime Mode transition progress API and UX
- GR-7 Observability over snapshot + event stream + server-side event history
- Playwright regression coverage for shell navigation, Now, Modes, Flowgraph Studio, and Observability

The remaining GUI roadmap is now mostly refinement work: visual polish, deeper domain-specific debugger views, and future backend integrations that are not yet specified.
