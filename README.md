# LocalDeck

**A project-centric control center for local development on macOS and Linux.**

LocalDeck discovers the development projects that are actually running on your machine, groups their processes and ports into a view you can reason about, remembers projects after they stop, and lets you inspect and control them from one native desktop app.

> Which projects are alive? Which component owns port `3000`? What is eating my CPU inside that project? Can I stop or restart it without hunting through terminals?

Operating-system tools answer those questions with unrelated PIDs and sockets. LocalDeck answers them with **projects** (`Project → Component → Process → Port`). It is not an Activity Monitor clone, a Docker manager, a log viewer or an observability stack, and it never needs an account or network access.

## Supported platforms

| Platform | Status |
|---|---|
| macOS (Apple Silicon / Intel) | Supported. Runtime-verified by the maintainers on macOS (see [Verification](#verification-status)). |
| Linux (X11/Wayland, `webkit2gtk-4.1`) | Supported; the code is conditionally compiled and unit-tested, but has **not** been runtime-verified. Tray behaviour depends on the desktop environment. |
| Windows | Out of scope for v0.1. |

## Features

- **Automatic discovery** — scans local processes and listening TCP sockets about every 2 seconds. Nothing to configure; start a dev server and it shows up.
- **Deterministic project roots** — nearest Git root (`.git` directory *or* worktree/submodule `.git` file) → workspace root (pnpm, Turborepo, Nx, Lerna, Go, Cargo, npm/Bun workspaces) → nearest manifest. A Tauri app with a nested `src-tauri/Cargo.toml` stays one project. No marker, no project.
- **Process trees, not PID soup** — `bun → sh → vite → esbuild` is one logical component. Interactive shells, terminal hosts and editor/language-server tooling are never mistaken for project runtime.
- **Stack and component detection** — React, Vue, Svelte, Vite, Next.js, Nuxt, Astro, Remix, Tauri, Bun, Node.js, TypeScript, Rust, PHP/Laravel, Python (Django/FastAPI/Flask), Go, Docker/Compose markers. Components are normalised to Web, API, Worker, Database, Cache, Queue, Bundler, Watcher, Desktop, CLI, Runtime or Other — and you can correct a classification.
- **Honest port ownership** — `Port → PID → Component → Project` only from PID/cwd/process-tree evidence. A global PostgreSQL or Redis stays *unassigned*; ambiguous sockets stay *ambiguous*. The Ports screen shows every listening endpoint, including unassigned ones, and answers “what owns port 3000?” immediately.
- **Safe HTTP detection** — known dev servers are recognised directly; otherwise only already-discovered loopback endpoints are probed with a short-timeout `HEAD`/`GET`. No range scanning, no external hosts, redirects are never followed.
- **Primary URL** — explicit override → Web component → known dev server → first confirmed HTTP endpoint. Wildcard listeners open as `localhost`.
- **Project actions** — open URL / folder / editor (VS Code, Cursor, Zed, JetBrains) / terminal; start, stop, restart; graceful terminate **separate from** an explicit force kill; `package.json` script picker that persists `program + args[] + cwd` and spawns without a shell, using the detected package manager.
- **Memory** — projects, overrides, favourites, ignored projects, settings and a bounded activity history (≈500 events) live in a Rust-owned SQLite database. CPU/RAM/process/socket samples are never persisted.
- **Native shell** — tray / menu-bar item with a running-project summary, launch at login, light/dark/system themes, global search and a command palette (`⌘K` on macOS, `Ctrl+K` on Linux).

## How it works

```text
React + TypeScript (presentation only)
  Projects · Project detail · Ports · Settings · Command palette
  Zustand: runtime projection + UI state
        │  Tauri commands (explicit actions)   ▲  event `runtime://updated` (full AppSnapshot ~2 s)
┌───────▼─────────────────────────────────────┴──────────┐
│ commands/   narrow, intention-revealing IPC              │
│ services/   runtime state, scanner thread, actions       │
│ engine/     process tree → project roots → grouping →    │
│             stack → components → ports → HTTP → URL      │
│ persistence/ SQLite (rusqlite) · platform/ OS actions    │
└──────────────────────────────────────────────────────────┘
```

- **Rust is authoritative.** React cannot scan processes or sockets, run arbitrary shell commands, touch SQLite or the filesystem. There is no `exec(string)` command; every command (`terminate_process`, `start_project`, `open_project_folder`, …) resolves paths and commands from Rust-owned state.
- **One scan at a time.** A dedicated scanner thread owns the cadence, so scans cannot overlap. Project roots and stack fingerprints are cached; the filesystem is never deep-scanned per tick; HTTP probes run only for new or changed project ports.
- **Third-party types stay behind LocalDeck models.** `sysinfo` and `netstat2` are wrapped by providers in `engine/providers.rs`.
- Types shared with the frontend are generated from the Rust models by `ts-rs` into `src/types/generated/` (regenerated by `cargo test`).

Design documents live in [`docs/`](docs/README.md); the canonical spec is [`docs/superpowers/specs/2026-10-04-localdeck-design.md`](docs/superpowers/specs/2026-10-04-localdeck-design.md).

## Development

### Prerequisites

- [Bun](https://bun.sh) ≥ 1.1
- Rust (stable, 2021 edition; developed against 1.99) via [rustup](https://rustup.rs)
- Tauri 2 system dependencies:
  - **macOS:** Xcode Command Line Tools (`xcode-select --install`).
  - **Linux:** `webkit2gtk-4.1`, `libayatana-appindicator3`, `librsvg2`, `patchelf` and a C toolchain — see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#linux).

### Setup and run

```bash
bun install            # JS dependencies
bun run tauri dev      # Vite + native app with hot reload (first Rust build takes a few minutes)
```

### Tests and checks

```bash
bun run build                                                    # tsc + vite build
bun test                                                         # frontend behaviour tests (Bun + happy-dom + Testing Library)
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml                  # discovery engine, persistence, platform and service tests
```

The Rust tests build their own fixture projects in temporary directories (spaces, non-ASCII paths, Git worktrees, monorepos, Tauri layouts) and never read your real home directory. A few service tests spawn short-lived `sleep`/`sh` processes to exercise real signalling and managed start/stop.

### Build the desktop app

```bash
bun run tauri build    # produces the native bundle for the host OS under src-tauri/target/release/bundle/
```

### Project layout

```text
src/                     React app (features/, components/, stores/, lib/, types/)
src-tauri/src/
  engine/                deterministic discovery and classification
  services/              runtime state, scanner, compose, activity tracker, actions
  persistence/           SQLite schema, migrations, bounded activity
  platform/              signals, open url/folder/editor/terminal, managed spawn (macOS + Linux)
  commands/              Tauri IPC surface
  tray.rs                tray / menu-bar item
```

## Known limitations

- **OS permissions limit what can be seen.** Processes owned by other users (and some system processes) expose only partial metadata, and their sockets may not report an owning PID. LocalDeck shows these as *limited access* / *owner not visible* instead of guessing. macOS App-Store-style sandboxing would restrict inspection further and is not a goal.
- **Start commands come from `package.json` scripts** in v0.1 (bun, pnpm, yarn or npm, chosen from `packageManager`/lockfiles; npm is assumed if nothing is found). Projects without a `package.json` can still be discovered, observed and stopped.
- **Stop is graceful and root-only by design.** It sends `SIGTERM` to the root process of each component. If anything survives, LocalDeck offers an explicit Force Stop; it never escalates silently.
- **Managed processes outlive LocalDeck.** Commands started from LocalDeck run in their own process group, like a terminal-launched job, and are rediscovered as external processes after a restart of LocalDeck.
- **Non-UTF-8 paths** (Linux only) are discovered correctly but displayed lossily; file actions on such a project may fail with a structured error.
- **Linux tray** support depends on the desktop environment (AppIndicator). Without one, the window simply behaves like a normal window and closing it quits the app.
- **Heuristics are explicit, not magic.** Editor/language-server processes, interactive shells and Homebrew/system locations are excluded from projects by fixed, reviewable lists in `engine/tooling.rs` and `engine/project_detector.rs`.

## Verification status

See the completion notes in the repository history for the exact commands run. In short: the Rust and frontend gates above were run on **macOS (Apple Silicon)** together with a native `tauri build` and a live smoke run of the app. The **Linux** code paths are compiled conditionally and covered by pure unit tests (command builders, editor/terminal tables), but were not executed on a Linux host.

## Non-goals (v0.1)

Windows, cloud/accounts/telemetry, AI classification, Docker/Kubernetes management, logs, an embedded terminal, `.env`/secret management, database or HTTP clients, network inspection, launch profiles, auto-restart supervision, plugins, a CLI, dependency graphs and historical metrics.
