# Security & Performance Constraints

## Threat model

LocalDeck is local software with unusually powerful capabilities:

- inspecting processes;
- discovering sockets;
- sending termination signals;
- spawning development commands;
- opening local files/apps/URLs.

The design must minimize the authority exposed to the webview.

## Tauri command surface

Expose task-specific commands.

Good:

```text
terminate_process(pid)
force_kill_process(pid)
start_project(project_id)
open_project_folder(project_id)
```

Bad:

```text
exec(any_string)
run_shell(command)
write_any_file(path, contents)
query_sql(sql)
```

Do not grant frontend shell/filesystem/database powers merely for convenience.

## Capabilities

Keep Tauri capabilities/permissions minimal.

Only add official plugins/capabilities required for implemented features.

Do not enable broad shell execution for frontend code.

## Process safety

Before destructive actions:

- look up the latest process model;
- handle a process disappearing between click and action;
- avoid targeting unrelated processes due to stale PID state;
- return a structured error on permission failure.

Normal Stop/Terminate must be graceful.

Force Kill must be user-explicit.

## Spawn safety

Persist normalized executable + args + cwd.

Do not interpolate user-controlled strings into a shell command unless a feature explicitly requires shell semantics and escaping has been designed.

v0.1 does not need arbitrary user shell scripts.

## HTTP probing

Only probe endpoints already discovered as local listening sockets.

Never scan port ranges, LAN hosts, or external addresses.

Use a short timeout and minimal request.

Do not follow arbitrary redirects to external networks as part of protocol detection; disable or tightly limit redirects.

## Filesystem reads

Project detection reads small project metadata files.

Do not recursively ingest source code.

Apply reasonable file-size limits before parsing metadata/manifests.

Treat parse errors as a detection miss/diagnostic, not an app crash.

## SQLite

- DB lives in application data.
- Use parameterized statements.
- Use migrations.
- Keep event retention bounded.
- Never store arbitrary process environment variables; they can contain secrets.

## Sensitive process data

Commands can contain tokens/secrets.

The UI may need command text for diagnostics, but:

- do not persist raw commands in long-lived activity history unless strictly necessary;
- do not log entire environment variables;
- avoid tracing sensitive arguments at info level.

No telemetry is required.

## Resource targets

These are engineering guardrails rather than hard benchmarking promises.

- scanner cadence ~2 seconds;
- no unbounded concurrent scans;
- no deep repository walks on every scan;
- no historical metric writes every scan;
- HTTP probes only on port changes/new ports;
- debounce/throttle last-seen persistence;
- keep React rerenders bounded using stable store selectors/components.

## Failure behavior

A failed subsystem should degrade gracefully where possible.

Examples:

- socket scanning failure → projects/processes may still display, with Ports diagnostic.
- SQLite transient failure → surface persistence failure; do not fabricate remembered state.
- inaccessible process cwd → process can remain partial/unassigned.
- HTTP probe timeout → port remains TCP/unknown rather than generating repeated errors.

Use tracing for developer diagnostics, but avoid noisy 2-second logs in normal operation.
