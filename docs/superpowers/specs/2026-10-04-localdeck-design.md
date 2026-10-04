# LocalDeck v0.1 Design Specification

**Date:** 2026-10-04  
**Repository:** `ogunbaysal/localdeck`  
**Platforms:** macOS and Linux  
**Status:** Approved product direction; implementation handoff

## 1. Summary

LocalDeck is a native, project-centric local development control center.

It automatically discovers development projects from real local processes, groups process trees and listening ports into project context, remembers projects after they stop, and provides fast inspection/control actions.

The app is built with:

- Tauri 2;
- Rust;
- React + TypeScript;
- Tailwind CSS;
- shadcn/ui;
- Zustand;
- Rust-owned SQLite.

The existing Tauri React TypeScript scaffold must be modified in place.

## 2. Core product decision

The product hierarchy is:

```text
Project → Component → Process → Port
```

Not:

```text
Process → maybe project
```

The main UI speaks in project names and logical components. PIDs/raw commands remain available for diagnostics.

## 3. First-run experience

There is no mandatory onboarding wizard.

At startup LocalDeck scans accessible local process/socket state.

If projects are running, they appear automatically.

If none are running:

```text
No running projects

Start one of your development projects
and LocalDeck will detect it automatically.

Previously discovered
○ ...
```

Automatic discovery is the product's first "wow" moment.

## 4. Project lifecycle

A project can be:

- **Running** — active attributed runtime processes exist.
- **Partial** — an active project is missing part of a previously-known/configured primary runtime; do not infer expectations from stack metadata alone.
- **Stopped** — remembered project, no active attributed processes.
- **Attention** — actionable issue/condition. This may be modeled orthogonally to runtime status internally.

LocalDeck remembers stopped projects through SQLite.

## 5. Discovery contract

Discovery is deterministic and local-only.

Engine pipeline:

```text
Process Scanner
→ Process Tree Builder
→ Project Root Detector
→ Project Grouper
→ Stack Detector
→ Component Classifier
→ Port Resolver
→ HTTP Probe
→ AppSnapshot
```

### Project root

Use cwd ancestor evidence.

Preferred root precedence:

1. nearest enclosing Git root (`.git` directory or worktree/submodule `.git` file);
2. otherwise nearest workspace root;
3. otherwise nearest meaningful standalone manifest root;
4. otherwise do not fabricate a project.

Workspace examples:

- pnpm workspace;
- Turborepo;
- Nx;
- Lerna;
- Go workspace;
- Cargo workspace.

Standalone markers include typical JS/Rust/PHP/Python/Go/Compose manifests.

A root Tauri app with nested `src-tauri/Cargo.toml` must remain one project when it shares the same Git root.

### Process trees

Children such as shell wrappers, Vite, esbuild, TypeScript watchers, etc. should be represented under logical command/component trees rather than as unrelated top-level project cards.

### Stack classification

Use project files/dependencies and process commands to identify useful labels.

No LLM classification.

### Ownership integrity

Do not attach an unrelated global PostgreSQL/Redis/etc. process to a project without cwd/process-tree evidence.

Global sockets can still appear on Ports as unassigned.

## 6. Ports

Discover listening local TCP endpoints and PIDs where supported.

Relationship:

```text
Port → PID → Component → Project
```

Preserve unassigned and ambiguous state.

For HTTP:

- use known dev-server evidence;
- optionally probe the already-discovered local endpoint;
- use a short timeout;
- never scan port ranges;
- do not probe external hosts;
- do not follow arbitrary external redirects.

Primary URL precedence:

1. explicit override;
2. web/frontend component HTTP port;
3. known dev-server HTTP port;
4. first confirmed project HTTP endpoint in deterministic order.

Convert wildcard listening addresses to a browser-usable loopback URL for display/open behavior when appropriate.

## 7. Components

Supported normalized kinds:

```text
Web
API
Worker
Database
Cache
Queue
Bundler
Watcher
Desktop
CLI
Runtime
Other
```

Classification uses executable/args/cwd/tree/stack/ports.

`Other` is preferable to an unjustified guess.

User correction/overrides must be possible for relevant classifications/settings.

## 8. Project start/stop

### Stop

Send graceful termination to appropriate attributed project root processes.

Do not silently escalate to force kill.

If processes remain, expose a separate explicit Force Kill path.

### Start

When a primary command exists, spawn it from the project root.

At minimum discover `package.json` scripts and the likely package manager.

Persist:

```text
program
args[]
cwd
```

Do not default to arbitrary shell execution.

Track whether a process is external or LocalDeck-managed.

### Restart

Restart is stop + managed start when a primary command exists. Handle failures explicitly.

## 9. Main UI

Top-level navigation:

```text
Projects
Ports
────────
Settings
```

There is no top-level Processes page.

### Projects

Cards show:

- project name/status;
- concise stack;
- important ports;
- process count;
- aggregated CPU/memory;
- primary action;
- overflow actions.

### Project detail

Single scrollable information hierarchy:

1. header/actions;
2. overview;
3. components;
4. ports;
5. activity;
6. collapsible process diagnostics.

### Ports

A global diagnostic table/list shows all discoverable listening endpoints, including unassigned ones.

It must make "what owns port 3000?" fast to answer.

### Search

Match projects, paths, ports, components/framework labels.

### Command palette

Use `⌘K` macOS / `Ctrl+K` Linux.

Expose actual actions: open/restart/stop/reveal/open URL/editor.

## 10. Tray

Use a native Tauri tray/menu-bar item.

It is a glance/quick-action surface, not a second full UI.

It should:

- summarize running projects;
- open LocalDeck;
- quit LocalDeck.

macOS and Linux tray behavior do not have to be pixel/interaction identical.

## 11. Settings

Required:

- system/light/dark theme;
- launch at login;
- preferred editor;
- remembered-project behavior;
- ignored projects management.

Global OS shortcut is optional.

## 12. Architecture

Rust is authoritative.

React must not directly:

- scan processes;
- inspect sockets;
- execute arbitrary shell;
- access SQLite;
- perform broad filesystem access.

Use Tauri:

- Commands for explicit request/response actions.
- Events for runtime snapshot updates.

A full `AppSnapshot` event every ~2 seconds is acceptable for v0.1; optimize to diffs only after evidence.

Use Zustand for:

- runtime projection;
- UI-only state.

Do not add Redux/TanStack Query without a demonstrated need.

## 13. Persistence

Rust-owned SQLite stores small durable state:

- known projects;
- overrides;
- settings;
- bounded activity.

Do not store time-series CPU/RAM/socket/process snapshots.

Keep activity bounded (~500 recent global events is acceptable).

Use migrations and the Tauri app data directory.

## 14. Platform abstraction

Initial preferred libraries:

- `sysinfo` for process/system facts;
- `netstat2` for socket facts/PID association;
- `rusqlite` for persistence;
- `serde`, `thiserror`, `tracing`.

Wrap third-party structures behind LocalDeck internal models/providers.

Keep macOS/Linux-specific action behavior behind platform/services.

## 15. Security

Frontend gets narrow business capabilities, not generic OS authority.

Never add generic frontend arbitrary shell execution.

Do not persist environment variables.

Be cautious with raw commands because they can contain secrets.

Before process signals, handle stale/disappearing PIDs and permission failures.

## 16. Performance

- background scan target around 2 seconds;
- prevent unbounded overlapping scans;
- cache project stack fingerprints;
- no deep filesystem scan each tick;
- HTTP probe only when relevant ports appear/change;
- throttle unnecessary SQLite writes;
- keep normal logs quiet.

## 17. Non-goals

v0.1 excludes:

- Windows;
- cloud/account/auth;
- required telemetry;
- AI/LLM;
- Docker management;
- Kubernetes;
- logs;
- embedded terminal;
- env/secrets;
- DB client;
- HTTP client;
- network inspection/tracing;
- launch profiles;
- auto-restart supervisor;
- plugins;
- CLI;
- dependency visualization;
- historical metrics.

## 18. Acceptance

The implementation is not complete when only the UI exists.

It must include:

- real process discovery;
- real project grouping;
- process tree handling;
- real listening port attribution where OS permits;
- persistence;
- real project/process actions;
- tray;
- settings;
- search/command palette;
- tests of deterministic core logic;
- successful applicable frontend/Rust build/test/lint gates;
- attempted native Tauri build on the implementation host.

See `docs/quality/01-testing-and-acceptance.md` for the detailed acceptance checklist.

## 19. Design philosophy

The desired outcome is not "Activity Monitor with nicer CSS."

A successful LocalDeck lets a developer think:

> "I can see exactly which development projects are alive, what they own, and control them without hunting through terminals."

When correctness and magic conflict, choose correctness.
