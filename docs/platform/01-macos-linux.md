# macOS & Linux Platform Design

## Supported platforms

v0.1:

- macOS
- Linux

Windows is explicitly out of scope.

## Shared-first approach

Prefer cross-platform crates/providers for process and socket facts, then isolate differences.

Do not scatter platform-specific branching across UI and engine algorithms.

## Process facts

Use a process provider backed initially by `sysinfo` where it supplies the required information.

Expect partial/missing metadata because:

- OS permissions differ;
- process lifetime races with scanning;
- some metadata is unavailable for processes owned by other users;
- macOS distribution/sandboxing can restrict process inspection.

The app must degrade to `Partial`/`Unavailable` access rather than panic.

## Socket facts

Use `netstat2` or an equivalent low-level cross-platform library to discover sockets without repeatedly shelling out to `lsof`.

- Linux implementation may use netlink/procfs internally.
- macOS implementation may use process socket APIs internally.
- Associated PIDs can be absent/ambiguous; preserve that state.

`lsof` may be used only as an explicitly justified diagnostic/fallback, not as the always-running primary scanner.

## Signals

### Graceful terminate

On Unix-like targets, normal terminate should request graceful shutdown (typically SIGTERM).

### Force kill

Force kill may use SIGKILL only after explicit user action.

A normal Stop action must not automatically escalate to SIGKILL after a timeout.

If graceful termination leaves processes alive, surface a follow-up Force Kill option.

## Project stop semantics

Do not signal every process on the machine whose cwd happens to match a string.

Use the current attributed process/component tree and select appropriate project root processes.

Guard against PID reuse/race conditions as reasonably possible by refreshing/checking before destructive actions.

## Spawning

LocalDeck-managed start commands should use:

```text
program + args[] + cwd
```

rather than an arbitrary shell string when possible.

Track spawned processes as `LocalDeckManaged`.

If a managed command exits unexpectedly, LocalDeck may create an Attention/activity event. Do not implement an auto-restart supervisor in v0.1.

## Open folder / editor / terminal

Hide OS-specific behavior behind services.

### Editor

Detect common installed editors where practical:

- Visual Studio Code
- Cursor
- Zed
- JetBrains IDEs when reliably discoverable

The user can select a preferred editor.

If an editor cannot be found, return a structured error and offer settings rather than executing unknown commands.

### Terminal

"Open Terminal" means open a system/user terminal at the project root.

Implementation may use OS conventions/app launching. Do not embed a terminal in LocalDeck.

### Browser

Use Tauri/OS opening mechanisms for confirmed primary URLs rather than executing arbitrary browser shell strings.

## Tray

Tauri tray support is acceptable for both desktop targets.

Important UX difference:

- macOS can provide the expected menu-bar experience.
- Linux tray capabilities vary by desktop environment and left-click menu behavior is not identical.

The main window must remain fully usable even when a Linux desktop does not provide the exact macOS tray interaction.

## Autostart

Use the official Tauri autostart plugin or current Tauri-supported equivalent.

Autostart must be optional and controllable in Settings.

## Distribution

Do not claim App Store compatibility as an MVP requirement. Full process inspection is a poor fit for strict sandbox distribution.

The implementation should produce the normal Tauri artifacts supported on the build host.

## File paths

Treat paths as paths, not UTF-8 assumptions.

- Rust APIs may expose OS strings.
- Serialize display-safe/lossy representations where necessary while retaining a usable internal path.
- Tests should include spaces and non-ASCII project names/paths.
