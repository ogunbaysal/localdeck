# Product Vision

## Problem

A developer commonly has several local projects running at once. Each project can create a tree of processes, several listening ports, file watchers, dev servers, workers, and auxiliary commands.

Operating-system tools expose these as unrelated PIDs and sockets. That answers low-level questions, but not the developer's actual questions:

- Which **projects** are currently running?
- Which ports belong to a project?
- What is consuming CPU or memory inside that project?
- Which component owns port `3000`?
- Can I open, stop, restart, or inspect the project without hunting through terminals?
- Which projects did I run recently even if they are stopped now?

LocalDeck solves those questions.

## Positioning

LocalDeck is **not** a replacement for Activity Monitor, `top`, `htop`, Docker Desktop, a terminal, a database client, or an observability stack.

It is a native **local development control center** whose primary entity is a project.

The core hierarchy is:

```text
Project
└── Component
    └── Process
        └── Port
```

A raw PID is diagnostic detail, not the product's home screen.

## Target user

Developers on macOS or Linux who:

- run multiple local web/backend/desktop projects;
- frequently use Node/Bun, Rust, PHP, Python, Go, or similar tooling;
- encounter port conflicts or forgotten dev servers;
- want a fast visual answer without memorizing `lsof`, `ps`, or platform-specific commands.

## Value proposition

### Zero configuration by default

Starting a development server should be enough for LocalDeck to discover the project.

### Project context instead of PID context

`node PID 18321` is less useful than:

```text
Anılarım
Web · Next.js
localhost:3000
```

### Local-first

There is no account, cloud backend, required telemetry, or remote processing in v0.1.

### Deterministic

Framework/project detection is based on files, process metadata, process trees, commands, and listening sockets. No LLM is needed.

### Correctness over magical guesses

If LocalDeck cannot reliably establish ownership, it should say so. In particular, a globally-running PostgreSQL or Redis instance must not be assigned to a project merely because that project probably uses it.

## Product principles

1. **Projects first.**
2. **Automatic until automatic becomes unreliable, then allow an override.**
3. **Show useful names before raw commands.**
4. **Make common actions one click or one command-palette action away.**
5. **Keep the dashboard glanceable.**
6. **Keep diagnostics available but secondary.**
7. **Never fake process ownership or action success.**
8. **Keep resource usage low enough that developers leave LocalDeck running.**
