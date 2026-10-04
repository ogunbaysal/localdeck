# v0.1 MVP Scope

## Required

### Project discovery

- Scan accessible processes on a fixed approximately 2-second cadence.
- Resolve a usable working directory when the OS exposes one.
- Detect project roots from Git/workspace/language markers.
- Group multiple related processes under one project.
- Recognize common monorepo/workspace markers.
- Remember previously discovered projects after they stop.
- Allow a project to be ignored.
- Allow display-name and primary-URL overrides.

### Process model

- PID and parent PID.
- Executable/name.
- Command/arguments when accessible.
- Working directory when accessible.
- CPU usage.
- Memory usage.
- Start time/uptime when accessible.
- Access state: full, partial, or unavailable.
- Parent/child process-tree awareness.
- Origin state when relevant: externally started or LocalDeck-managed.

### Components

Normalize related process trees into logical components.

Minimum component kinds:

- Web
- API
- Worker
- Database
- Cache
- Queue
- Bundler
- Watcher
- Desktop
- CLI
- Runtime
- Other

Component classification may be imperfect, so overrides must remain possible. Do not display confidence scores to users.

### Stack/framework detection

Detect common technologies from project files/dependencies, including enough coverage to make the app useful for typical projects:

- React
- Vite
- Next.js
- Nuxt
- Astro
- Tauri
- Bun
- Node.js
- TypeScript
- Rust/Cargo
- Laravel/PHP
- Python projects (`pyproject.toml`)
- Go modules/workspaces
- Docker/Compose marker presence

Docker presence is only stack metadata in v0.1; Docker management is out of scope.

### Ports

- Discover listening TCP sockets and associated PIDs when the OS permits.
- Associate ports through PID -> component -> project.
- Support IPv4 and IPv6.
- Show unassigned listening ports when a reliable project relationship does not exist.
- Identify likely HTTP ports.
- Probe only already-discovered local listening ports with a short timeout.
- Never perform broad localhost/network scanning.
- Choose a primary URL using deterministic heuristics plus user override.

### Project persistence

Persist:

- root path;
- display name;
- first/last seen;
- favorite flag if implemented in UI;
- ignored flag;
- primary URL override;
- primary start command;
- preferred editor override if project-specific;
- bounded activity events.

Do not persist continuous CPU/RAM/process/socket samples.

### Project actions

Required actions:

- Open primary URL.
- Copy URL.
- Open project folder.
- Open project in configured editor.
- Open a terminal at project root using platform behavior.
- Gracefully terminate an individual process.
- Force-kill an individual process after explicit user action.
- Stop a project by terminating LocalDeck's selected project root processes.
- Restart a project when a usable primary start command is known.
- Start a stopped project when a usable primary start command is known.

A normal Stop must not silently escalate to force-kill.

### Start commands

v0.1 must at minimum:

- discover `package.json` scripts;
- determine the package manager from the project/lockfiles where practical;
- let the user select a script as the primary start command;
- persist the resulting executable/arguments/cwd representation;
- spawn without an arbitrary shell where normal executable + arguments are sufficient.

Supporting additional command sources is allowed only if it does not delay core acceptance criteria.

### UI

Required routes/surfaces:

- Projects dashboard.
- Project detail.
- Global Ports screen.
- Settings.
- Command palette.
- Native tray/menu-bar menu.

Required dashboard states:

- Running.
- Partial.
- Stopped.
- Attention.

Required project detail information:

- status;
- detected stack;
- root path;
- ports;
- aggregated CPU/memory;
- components;
- collapsible process diagnostics;
- recent activity.

### Search / command palette

Search should match at least:

- project name;
- root path;
- port;
- component/framework text.

Command palette must expose useful project actions rather than acting as a decorative search dialog.

### Settings

Required:

- theme: system/light/dark;
- launch at login;
- preferred editor;
- remembered projects behavior;
- ignored projects management.

A global shortcut is optional for v0.1 and must not block release.

### Activity

Store meaningful transitions only:

- project started;
- project stopped;
- process/component crash when inferable;
- port opened;
- port closed;
- port conflict/attention event.

Keep storage bounded; the default implementation should retain only a small recent history (for example ~500 events globally), not an audit archive.

## Explicit non-goals for v0.1

Do **not** implement:

- Windows support.
- Cloud sync.
- Accounts/authentication.
- Required analytics/telemetry.
- LLM/AI classification.
- Docker container management.
- Kubernetes.
- Log viewer.
- Embedded terminal emulator.
- `.env`/secret manager.
- Database client.
- HTTP/API client.
- Network packet inspection.
- Request tracing.
- Multi-command launch profiles.
- Automatic process restart/supervision.
- Plugin/extension system.
- Separate LocalDeck CLI.
- Dependency graph visualization.
- Historical metrics/time-series monitoring.

These can be future roadmap items. They are not reasons to enlarge the initial architecture.
