# LocalDeck Documentation

LocalDeck is a **project-centric local development control center** for macOS and Linux.

This directory is designed to be sufficient for an implementation agent that has never seen the earlier product discussion.

## Documentation map

| Document | Purpose |
|---|---|
| `superpowers/specs/2026-10-04-localdeck-design.md` | Canonical product + technical design |
| `product/01-product-vision.md` | Product positioning and principles |
| `product/02-mvp-scope.md` | Exact v0.1 scope and non-goals |
| `product/03-ux-information-architecture.md` | Screens, navigation, interactions |
| `engine/01-discovery-and-classification.md` | Deterministic project/process/port intelligence |
| `architecture/01-system-architecture.md` | Rust/React boundaries and module structure |
| `architecture/02-ipc-state-persistence.md` | Tauri IPC, Zustand projection, SQLite |
| `platform/01-macos-linux.md` | Platform abstractions and OS-specific behavior |
| `quality/01-testing-and-acceptance.md` | Tests and release acceptance criteria |
| `quality/02-security-and-performance.md` | Capability, process safety, performance constraints |
| `reference/01-technical-references.md` | External technical references |

## Product in one sentence

> LocalDeck automatically discovers local development projects that are running on a developer's machine, groups their processes and ports into understandable project views, remembers stopped projects, and lets the developer inspect and control them from one native desktop app.

## Repository assumption

The app has already been scaffolded using:

```bash
bun create tauri-app@latest localdeck -- --template react-ts --manager bun
```

The implementation must evolve that repository instead of re-scaffolding it.

## Precedence

When requirements appear ambiguous:

1. `AGENTS.md`
2. Canonical design spec
3. MVP scope
4. Architecture/engine/UX documents
5. Reasonable implementation judgment

Do not expand the scope simply because a possible enhancement is easy.
