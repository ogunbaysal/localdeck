# Testing & Acceptance Criteria

## Testing strategy

The highest-value tests are deterministic Rust unit/integration tests for discovery logic.

### Rust unit/integration coverage

Required behavior classes:

1. Project-root detection.
2. Git worktree/submodule `.git` file handling.
3. Monorepo/workspace root selection.
4. Tauri root project vs nested `src-tauri/Cargo.toml`.
5. No project marker → no fabricated project.
6. Process parent/child tree construction.
7. Separate sibling command trees remain separate.
8. Component classification for representative known commands.
9. Unknown component falls back safely.
10. PID-to-port-to-project association.
11. Ambiguous/unassigned socket ownership is preserved.
12. Primary URL selection and user override precedence.
13. `0.0.0.0` listening URL presentation becomes a browser-usable loopback URL where appropriate.
14. Primary command normalization from `package.json` scripts.
15. Package-manager detection from lockfiles.
16. SQLite initialization/migrations.
17. Bounded activity retention.
18. Structured error serialization.
19. Path handling with spaces/non-ASCII.
20. Missing permissions/metadata does not panic.

Use fixture directories/tempdirs rather than depending on the developer's actual home directory.

### Frontend tests

Use Vitest + React Testing Library (or the existing equivalent) for behavior, not pixel snapshots.

At minimum test:

- project cards render running/stopped states;
- search can match project names and ports;
- project detail renders components/ports;
- destructive force-kill requires explicit action;
- command palette surfaces project actions;
- empty state correctly shows remembered projects;
- structured backend errors render a meaningful state.

### E2E/manual smoke

A small manual/native smoke checklist is acceptable because OS process behavior is hard to fully virtualize.

## Acceptance criteria — core

The v0.1 implementation is complete only when all of the following are true.

### A. Existing scaffold preserved

- The original Tauri + React repository has been evolved in place.
- No nested replacement app was created.

### B. Real discovery

With a representative dev server running from a Git project:

- LocalDeck discovers the real process without seeded/mock production data.
- It identifies a project root.
- The project appears on Projects.
- CPU/memory/process count come from the backend.
- The real listening port appears when PID association is available.

### C. Monorepo/Tauri grouping

Given a project such as:

```text
localdeck/
├── .git/
├── package.json
└── src-tauri/
    └── Cargo.toml
```

processes inside both root frontend and `src-tauri` resolve to the same LocalDeck project unless an actual nested Git root establishes otherwise.

### D. Process tree

A command tree such as `bun → sh → vite → esbuild` is not rendered as four top-level project components.

Raw processes remain inspectable.

### E. Port ownership integrity

- A project port is linked through an owning PID/process.
- A global PostgreSQL/Redis socket with no project evidence remains unassigned.
- The global Ports screen still shows unassigned sockets when available.

### F. HTTP/primary URL

For a normal Vite/Next-style local dev server:

- LocalDeck recognizes or safely confirms HTTP.
- `Open` uses a browser-friendly URL.
- A user primary-URL override wins over automatic selection.

### G. Persistence

After LocalDeck sees a project and that project stops:

- reopening/restarting LocalDeck still shows it as a remembered stopped project;
- ignored projects remain ignored;
- project overrides persist.

Continuous CPU/RAM samples are not stored historically.

### H. Actions

Where current-user permissions allow:

- Open URL works.
- Open Folder works.
- Open Editor works after a valid preferred editor is configured/detected.
- Open Terminal opens at project root.
- Process Terminate requests graceful shutdown.
- Force Kill is separate and explicit.
- Project Stop acts on attributed root process trees.
- Start/Restart work when a primary command is configured.

An action that fails must return/display failure; it must not optimistically claim success indefinitely.

### I. Primary command

For a project with `package.json`:

```json
{
  "scripts": {
    "dev": "vite"
  }
}
```

LocalDeck can expose `dev`, let the user choose it as primary, persist that choice, and spawn it using the detected package manager with the project root as cwd.

### J. UI

- Projects is the default route.
- Ports and Settings exist.
- No top-level Processes route exists.
- Project detail is project-first and exposes process diagnostics secondarily.
- Search works.
- Command palette works.
- Light/dark/system themes work.
- Layout remains usable at a compact desktop window size.

### K. Tray

- A native tray/menu-bar icon exists on the current build platform.
- It can open the main window and quit.
- It lists a compact running-project summary where supported.
- Main functionality does not depend on Linux reproducing macOS menu-bar semantics exactly.

### L. Performance behavior

- Background scans do not intentionally overlap without bound.
- Filesystem stack detection is cached/incremental rather than deep-scanned every 2 seconds.
- HTTP probing occurs only for relevant discovered ports.
- The app remains responsive during background scans.

### M. Quality gate

The final repository has no known failing tests/build checks applicable to the host.

Run equivalents of:

```bash
bun run build
bun test
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
bun run tauri build
```

If `bun test` does not exist initially, add an appropriate test script when frontend tests are introduced.

## Completion evidence

Before the implementing agent finishes, it should summarize:

- features implemented;
- verification commands actually run and results;
- any platform that was not runtime-tested;
- any known limitation caused by OS permissions;
- any acceptance criterion not met.

It must not describe an untested Linux build as verified merely because macOS passed, or vice versa.
