# IPC, State & Persistence

## Source of truth

Rust owns canonical runtime state.

React maintains a projection for rendering, not an independent competing interpretation.

## Runtime snapshot

A practical conceptual model:

```text
AppSnapshot
- projects[]
- ports[]
- scanner_status
- updated_at
```

Project concept:

```text
Project
- id
- name
- root_path
- runtime_status
- attention[]
- stack[]
- components[]
- primary_url?
- metrics
- first_seen_at
- last_seen_at
```

Component concept:

```text
Component
- id
- name
- kind
- technology?
- process_ids[]
- ports[]
- status
- metrics
```

Process concept:

```text
ProcessInfo
- pid
- parent_pid?
- name
- executable?
- args[]
- cwd?
- cpu_usage
- memory_bytes
- started_at?
- access_level
- origin
```

Port concept:

```text
PortInfo
- address
- port
- transport
- pid(s)
- project_id?
- component_id?
- application_protocol?
- is_http
- url?
```

Use concrete Rust/TypeScript types generated or kept in sync by tests/compile checks. Do not rely on `any`.

## Tauri IPC

### Commands

Commands are for explicit request/response actions:

```text
get_snapshot
get_settings
update_settings
set_project_override
ignore_project
set_primary_command
start_project
stop_project
restart_project
terminate_process
force_kill_process
open_project_url
open_project_folder
open_project_editor
open_project_terminal
```

Names may follow Rust/Tauri conventions, but keep commands narrow and intention-revealing.

Never expose a generic `exec(command: string)` command to the frontend.

### Events

For v0.1 prefer a simple runtime update event:

```text
runtime://updated
```

Payload may be the current snapshot.

At startup:

```text
React → get_snapshot()
Rust  → snapshot
React subscribes to runtime://updated
```

A full local snapshot every ~2 seconds is acceptable for v0.1 at expected desktop scale. Do not create a complex diff protocol before profiling proves it necessary.

Additional small events are acceptable for actionable one-off notifications, but avoid two competing update mechanisms.

## Zustand

Use two conceptual stores.

### Runtime store

Contains Rust-owned projection:

- projects;
- ports;
- scanner status;
- updated timestamp.

### UI store

Contains frontend-only state:

- selected filters;
- search;
- sidebar state;
- command-palette state;
- expanded/collapsed diagnostics.

Do not persist transient runtime metrics through frontend local storage.

## SQLite ownership

SQLite is Rust-owned.

Do not give React direct SQL access, even though Tauri plugins can technically make that possible.

Use `rusqlite` or an equivalently small Rust-side solution.

## Data model

A minimal schema can use normalized tables or a small amount of JSON for override payloads where appropriate.

Required persisted concepts:

### projects

```text
id
root_path UNIQUE
display_name_override nullable
first_seen_at
last_seen_at
favorite
ignored
```

### project_overrides

At least:

```text
project_id
primary_url nullable
primary_command nullable
preferred_editor nullable
component_overrides nullable
```

The exact physical schema may split fields into dedicated tables if that is cleaner.

### activity

```text
id
project_id nullable
event_type
metadata
created_at
```

### settings

Either explicit columns in a singleton row or typed key/value storage.

## Activity retention

Bound storage.

A simple acceptable rule is to keep the latest ~500 global events and prune older rows transactionally after inserts/batches.

No historical metrics table is required.

## Database migrations

The app must initialize/migrate its database predictably.

- Store the DB in the normal Tauri application data directory.
- Do not write app data into the project source tree.
- Migrations must be testable against an empty temporary DB.
- A database failure must not be reported as successful project discovery.

## Persistence timing

Write when meaningful data changes:

- project first discovered;
- last-seen updates at a reasonable throttled cadence, not every scanner tick if avoidable;
- override changed;
- setting changed;
- activity transition emitted.

Do not write every CPU/memory update.
