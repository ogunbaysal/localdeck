# UX & Information Architecture

## Navigation

Keep top-level navigation deliberately small:

```text
Projects
Ports
────────
Settings
```

There is no top-level Processes route in v0.1.

## Projects dashboard

`Projects` is the home/dashboard.

Example information hierarchy:

```text
LocalDeck                                  Search / Command

Running

● LocalDeck
  React · Tauri · Rust
  :1420 · 4 processes
  4.2% CPU · 286 MB
  Open                                      Stop ···

● Anılarım
  Next.js · Bun
  :3000 · :4000 · 5 processes
  7.8% CPU · 620 MB
  Open                                      Stop ···

Stopped

○ HealthNode
○ XON CRM
```

### Card rules

Show:

- project name/status;
- concise technology summary;
- main ports;
- process count;
- aggregate CPU and memory;
- one primary action plus a compact overflow menu.

Do not put full process tables, long raw commands, or dense charts on project cards.

## Project detail

Prefer one scrollable detail page over a tab-heavy interface.

Recommended order:

1. Header/actions.
2. Overview metrics.
3. Components.
4. Ports.
5. Recent Activity.
6. Collapsible Processes diagnostic section.

Example:

```text
← Projects

LocalDeck                                      ● Running
React · Tauri · Rust
~/Projects/localdeck

[ Open ] [ Restart ] [ Stop ] [ ··· ]

Ports             CPU             Memory
:1420             4.2%            286 MB

Components

● Frontend
  Vite · :1420                      124 MB

● Desktop
  Tauri                             138 MB

Activity
18:04  Vite started
18:04  Port 1420 opened

▶ Processes (4)
```

## Global Ports screen

This retains the original port-inspection value while keeping LocalDeck project-centric.

Columns:

```text
PORT    PROJECT       COMPONENT      PROCESS       TYPE
1420    LocalDeck     Frontend       vite          HTTP
3000    Anılarım      Web            next          HTTP
5432    —             —              postgres      TCP
```

The PostgreSQL row above is intentionally unassigned when LocalDeck cannot prove project ownership.

Selecting a port should reveal:

- local address;
- protocol/state;
- owning PID(s) when available;
- project/component if known;
- URL if HTTP;
- open/copy/reveal/terminate actions as applicable.

The screen must answer "what owns port 3000?" immediately.

## Status model

### Running

The project has active runtime processes/components.

### Partial

The project is active but a previously-known/expected primary component is absent or only a subset of its known runtime is active. Do not fabricate "expected" components solely from stack metadata.

### Stopped

The project is remembered but currently has no attributed running processes.

### Attention

A meaningful actionable condition exists, such as:

- port conflict/ambiguous ownership;
- a LocalDeck-managed command exited unexpectedly;
- process action failed due to permissions;
- a known primary runtime disappeared unexpectedly.

Attention must supplement, not replace, the runtime state internally if the implementation benefits from separate state fields.

## Actions

Project overflow menu may contain:

- Open URL.
- Open Folder.
- Open in preferred editor.
- Open Terminal.
- Start.
- Restart.
- Stop.
- Project details/settings.

Individual process actions:

- Terminate.
- Force Kill.
- Copy command.
- Open working directory.

Force Kill must be visually/descriptively distinct from graceful termination.

## Search

Dashboard search should match:

```text
localdeck
3000
localhost:3000
next
~/Projects/localdeck
```

## Command palette

Keyboard:

- macOS: `⌘K`
- Linux: `Ctrl+K`

Examples:

```text
Open LocalDeck
Restart Anılarım
Stop PUQ
Open localhost:3000
Reveal process on :5173
Open Anılarım in Cursor
```

A separate OS-global shortcut is optional.

## Tray/menu bar

### macOS

Use a native tray/menu-bar item as a glance/quick-action surface.

### Linux

Use tray support where the environment supports it, but do not depend on identical left-click semantics across desktop environments.

Tray content should remain compact:

```text
LocalDeck
────────────
● LocalDeck     :1420
● Anılarım      :3000 :4000
────────────
Open LocalDeck
Quit
```

Do not build a second complex application inside the tray for v0.1.

## Empty state

When no project is running:

```text
No running projects

Start one of your development projects
and LocalDeck will detect it automatically.

Previously discovered
○ LocalDeck
○ Anılarım
```

Manual project addition may exist as a secondary escape hatch, never as the main onboarding path.

## Visual direction

Use shadcn/ui as primitives, not as a visual identity by itself.

Direction:

- compact;
- native-feeling;
- subtle borders;
- restrained shadows;
- small/moderate radii;
- light and dark themes;
- system sans-serif for UI;
- monospace for paths, commands, PIDs, and ports;
- minimal decorative gradients/glassmorphism.

The interface should feel closer to a focused developer utility than a marketing dashboard.
