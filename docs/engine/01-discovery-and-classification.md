# Discovery & Classification Engine

## Goal

Convert raw OS facts into a deterministic runtime model:

```text
OS Processes + Listening Sockets
          ↓
Process tree
          ↓
Project roots
          ↓
Project grouping
          ↓
Stack fingerprints
          ↓
Logical components
          ↓
Port ownership
          ↓
HTTP/primary URL
          ↓
AppSnapshot
```

## Scan cadence

Default target:

- process refresh: approximately every 2 seconds;
- socket refresh: approximately every 2 seconds;
- CPU/memory refresh: approximately every 2 seconds;
- HTTP probe: only for newly-observed/changed relevant listening ports;
- filesystem metadata/stack detection: when a project is first discovered or its fingerprint is invalidated.

Do not recursively re-scan every project directory every 2 seconds.

## Raw process normalization

A normalized process should capture, where the OS permits:

```text
pid
parent_pid
name
executable
arguments/command
cwd
cpu_usage
memory_bytes
started_at
access_level
origin
```

`access_level`:

```text
Full
Partial
Unavailable
```

`origin`:

```text
External
LocalDeckManaged
```

Do not treat missing metadata as proof a process is irrelevant.

## Project-root detection

For each accessible process with a meaningful cwd:

1. Walk cwd ancestors upward and collect project-root evidence.
2. Stop at filesystem root; avoid uncontrolled recursion into unrelated directories.
3. Treat `.git` directories **and `.git` files** as Git-root evidence (important for worktrees/submodules).
4. Recognize workspace roots such as:
   - `pnpm-workspace.yaml`
   - `turbo.json`
   - `nx.json`
   - `lerna.json`
   - `go.work`
   - Cargo workspace declarations
5. Recognize standalone project manifests such as:
   - `package.json`
   - `Cargo.toml`
   - `composer.json`
   - `pyproject.toml`
   - `go.mod`
   - `docker-compose.yml`
   - `compose.yml`

### Selection rule

Prefer the most defensible root, not simply the closest manifest.

Recommended deterministic precedence:

1. nearest enclosing Git root when present;
2. otherwise nearest enclosing workspace root;
3. otherwise nearest meaningful standalone manifest root;
4. otherwise no automatic project.

This makes a typical Tauri app with root `package.json` and nested `src-tauri/Cargo.toml` one project when both are under the same Git root.

### Exceptions and overrides

- A nested Git repository/submodule is its own root when the process is inside it.
- Users can override/ignore an incorrect discovery.
- Unit tests must cover Git worktree `.git` files, monorepos, nested manifests, and no-marker directories.

## Project naming

Priority:

1. explicit LocalDeck display-name override;
2. root `package.json` name;
3. root Cargo package/workspace name when meaningful;
4. Composer project name;
5. Python project name;
6. Git/repository/root directory basename.

Humanize package-style names for display without changing the persisted root identity.

## Project identity

Do not key a project only by display name.

Use a stable ID derived from persisted DB identity, with normalized/canonical root path as the uniqueness basis.

Handle symlinks consistently; prefer canonical paths when canonicalization is available, but do not make an inaccessible canonical path crash discovery.

## Process-tree grouping

Build parent/child relationships from PID/PPID.

Example:

```text
bun run dev
└── sh
    └── vite
        └── esbuild
```

The UI should normally see this as one logical frontend component with an expandable process tree, not four unrelated cards.

A component-group root is generally the highest project-attributed process whose parent is not attributed to the same logical command tree. Classification may inspect descendants to determine the best technology label.

Do not merge unrelated sibling process trees merely because they share a cwd.

## Component classification

Input evidence may include:

- executable;
- arguments;
- cwd relative to project root;
- ancestors/descendants;
- listening ports;
- stack fingerprint;
- script/command name.

Normalize to:

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

Examples of useful deterministic clues:

```text
next dev                 → Web / Next.js
vite                     → Web or Bundler / Vite
tauri dev/runtime        → Desktop / Tauri
php artisan queue:work   → Worker / Laravel
tsc --watch              → Watcher / TypeScript
```

Avoid overclaiming API vs Web when the evidence does not support the distinction. `Other` is acceptable.

Internally a classifier may produce a confidence value, but do not display numeric confidence to the user.

## Stack fingerprint

Inspect small known files rather than the entire repository.

Examples:

- dependency maps in `package.json`;
- `vite.config.*`;
- `next.config.*`;
- `nuxt.config.*`;
- `astro.config.*`;
- `src-tauri`;
- `Cargo.toml`;
- `composer.json`;
- `artisan`;
- `pyproject.toml`;
- `go.mod` / `go.work`;
- Compose markers.

Cache the result by project root and invalidate only when relevant markers change or the user requests refresh.

## Socket/port ownership

Use OS socket information to obtain listening endpoints and associated PIDs where possible.

Relationship:

```text
ListeningPort
→ PID
→ ProcessTree/Component
→ Project
```

Support IPv4 and IPv6.

When multiple PIDs are reported, preserve the ambiguity instead of arbitrarily selecting one.

### No guessed infrastructure ownership

A globally-running process such as:

```text
postgres :5432
redis    :6379
```

must remain unassigned unless process-tree/cwd evidence ties it to a project.

It may still appear in the global Ports screen.

## HTTP detection

Do not treat every TCP port as a browser URL.

Order of evidence:

1. known HTTP dev-server/framework/process heuristic;
2. safe local HTTP probe of the already-discovered listening endpoint.

Probe constraints:

- only loopback/local endpoints already discovered;
- short timeout, target around 200–500 ms;
- no broad port range scanning;
- HEAD is preferred when useful, with a minimal GET fallback if required;
- failure means "not confirmed HTTP", not an application error.

Do not probe known database/debug protocols solely for curiosity.

## Primary URL selection

Priority:

1. explicit user override;
2. HTTP endpoint owned by a Web/Frontend component;
3. known dev-server HTTP endpoint;
4. first confirmed project HTTP endpoint in stable deterministic order;
5. none.

Use loopback-friendly display URLs. Do not expose `0.0.0.0` as the preferred browser host when `localhost` is appropriate.

## Primary start command

At minimum read `package.json` scripts.

Determine package-manager preference from evidence such as:

- `bun.lock` / `bun.lockb`;
- `pnpm-lock.yaml`;
- `yarn.lock`;
- `package-lock.json`.

Let the user select a script. Persist the normalized spawn representation:

```text
program
args[]
cwd
```

For example:

```text
program = "bun"
args = ["run", "dev"]
cwd = "/.../localdeck"
```

Do not default to `sh -c`/`bash -c` for ordinary commands.

## Snapshot comparison

Each scan produces or updates an in-memory `AppSnapshot`.

Compare previous and current normalized state to derive meaningful events:

- project transitions started/stopped;
- relevant port opened/closed;
- LocalDeck-managed command exited unexpectedly;
- actionable ambiguity/conflict.

Avoid event spam from metrics changes.

## Explainability

Detection rules should be unit-testable and, where useful, able to expose internal reasons for diagnostics.

Never require a remote service or model to decide what a local project is.
