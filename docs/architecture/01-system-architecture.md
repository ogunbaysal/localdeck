# System Architecture

## Principle

> React renders LocalDeck. Rust *is* LocalDeck.

The core discovery/action/persistence logic must remain useful without React so a future CLI could reuse it. A CLI is not part of v0.1.

## High-level architecture

```text
┌────────────────────────────────────────────┐
│ React + TypeScript                         │
│                                            │
│ Projects / Project Detail / Ports          │
│ Settings / Search / Command Palette        │
│ Zustand runtime projection + UI state      │
└────────────────────┬───────────────────────┘
                     │ Tauri Commands/Events
┌────────────────────▼───────────────────────┐
│ Rust Application Layer                     │
│                                            │
│ Project actions / process actions/settings │
├────────────────────────────────────────────┤
│ Discovery Engine                           │
│ process → tree → project → component/port  │
├────────────────────────────────────────────┤
│ Providers / Platform                       │
│ processes | sockets | open/spawn/signals   │
├────────────────────────────────────────────┤
│ SQLite Persistence                         │
└────────────────────────────────────────────┘
```

## Recommended Rust layout

Adapt to the generated Tauri 2 scaffold instead of forcing filenames that conflict with it.

```text
src-tauri/src/
├── lib.rs
├── state.rs
├── commands/
│   ├── mod.rs
│   ├── projects.rs
│   ├── processes.rs
│   ├── ports.rs
│   ├── actions.rs
│   └── settings.rs
├── engine/
│   ├── mod.rs
│   ├── scanner.rs
│   ├── process_tree.rs
│   ├── project_detector.rs
│   ├── stack_detector.rs
│   ├── classifier.rs
│   ├── port_resolver.rs
│   └── http_probe.rs
├── models/
│   ├── mod.rs
│   ├── project.rs
│   ├── component.rs
│   ├── process.rs
│   ├── port.rs
│   ├── activity.rs
│   └── settings.rs
├── services/
│   ├── mod.rs
│   ├── project_service.rs
│   ├── process_service.rs
│   └── action_service.rs
├── persistence/
│   ├── mod.rs
│   ├── database.rs
│   ├── projects.rs
│   ├── overrides.rs
│   └── settings.rs
└── platform/
    ├── mod.rs
    ├── macos.rs
    └── linux.rs
```

This is a responsibility guide, not a demand for empty abstraction files. Combine modules when the resulting file remains focused.

## Provider boundaries

Do not couple engine algorithms directly to a third-party crate's structs.

Use narrow internal interfaces/converters such as:

```rust
trait ProcessProvider {
    fn snapshot(&mut self) -> Result<Vec<RawProcess>, AppError>;
}

trait SocketProvider {
    fn listening_sockets(&self) -> Result<Vec<RawSocket>, AppError>;
}
```

Platform action boundaries should cover needs such as:

```text
graceful terminate
force kill
open folder
open editor
open terminal
spawn managed command
```

Exact Rust signatures may evolve during implementation, but keep OS details out of React.

## Suggested frontend layout

```text
src/
├── app/
│   ├── router.tsx
│   └── providers.tsx
├── features/
│   ├── projects/
│   ├── ports/
│   ├── command-palette/
│   └── settings/
├── components/
│   ├── ui/
│   └── shared/
├── stores/
│   ├── runtime-store.ts
│   └── ui-store.ts
├── lib/
│   ├── tauri.ts
│   └── format.ts
└── types/
```

Use shadcn components under the normal `components/ui` convention when its tooling establishes that convention.

## Rust libraries

Preferred responsibilities:

- `sysinfo`: process/system information.
- `netstat2`: cross-platform socket discovery/PID association where supported.
- `rusqlite`: SQLite owned by Rust.
- `serde`: IPC serialization.
- `thiserror`: structured internal errors.
- `tracing`: diagnostics.
- an async HTTP client compatible with the existing runtime (for loopback HTTP probing).

Do not blindly hard-pin versions from this document. Select current versions compatible with the repository Rust/Tauri toolchain and lock them normally.

## Frontend libraries

- React.
- TypeScript.
- Tailwind CSS.
- shadcn/ui.
- Zustand.
- React Router if routing improves structure.
- Lucide icons.

Do not add Redux or TanStack Query unless implementation evidence demonstrates a requirement missing from the approved design.

## Background engine

A background Rust task owns periodic discovery.

Concept:

```text
tick
 ↓
refresh process facts
refresh listening sockets
 ↓
normalize
 ↓
build process tree
 ↓
resolve project ownership
 ↓
classify components
 ↓
aggregate metrics/ports
 ↓
compare prior snapshot
 ↓
update in-memory AppSnapshot
 ↓
emit runtime update
 ↓
persist only meaningful transitions/metadata
```

Avoid overlapping scans: if a scan takes longer than one interval, do not create an unbounded queue of concurrent scans.

## Structured errors

Do not expose arbitrary debug strings as the entire error contract.

Representative codes:

```text
PROCESS_NOT_FOUND
PROCESS_PERMISSION_DENIED
PROJECT_NOT_FOUND
PORT_SCAN_FAILED
COMMAND_SPAWN_FAILED
DATABASE_ERROR
UNSUPPORTED_OPERATION
```

Errors should retain a human-readable message and optional safe context.

## Dependency direction

Preferred:

```text
React
  ↓
Tauri commands/events
  ↓
services
  ↓
engine / persistence / platform providers
```

Avoid:

```text
React → SQLite
React → arbitrary shell
React → OS process APIs
engine → React concepts
```
