# LocalDeck — Agent Instructions

This repository is **LocalDeck**, a project-centric local development control center for macOS and Linux.

## Read first

Before changing code, read these files in order:

1. `docs/README.md`
2. `docs/superpowers/specs/2026-10-04-localdeck-design.md`
3. `docs/product/01-product-vision.md`
4. `docs/product/02-mvp-scope.md`
5. `docs/product/03-ux-information-architecture.md`
6. `docs/engine/01-discovery-and-classification.md`
7. `docs/architecture/01-system-architecture.md`
8. `docs/architecture/02-ipc-state-persistence.md`
9. `docs/platform/01-macos-linux.md`
10. `docs/quality/01-testing-and-acceptance.md`
11. `docs/quality/02-security-and-performance.md`
12. `docs/reference/01-technical-references.md`

The canonical product/design source of truth is:
`docs/superpowers/specs/2026-10-04-localdeck-design.md`.

If another document conflicts with the canonical spec, follow the canonical spec.

## Existing repository

The repository is already scaffolded with:

```bash
bun create tauri-app@latest localdeck -- --template react-ts --manager bun
```

Git is already initialized.

**Do not re-scaffold the project, replace `.git`, or create a second app inside the repository.**
Inspect the generated files first and evolve the existing scaffold in place.

## Required stack

- Tauri 2
- Rust backend/core
- React
- TypeScript with strict typing
- Tailwind CSS
- shadcn/ui
- Bun for JavaScript package management
- SQLite owned by the Rust layer
- macOS and Linux only for v0.1

Use current dependency versions compatible with the repository's Rust toolchain and Tauri version. Do not upgrade unrelated dependencies without a concrete need.

## Architecture rules

1. **Project-centric, not process-centric.** Projects are the primary UI/domain entity.
2. **Rust owns system access.** React must not directly scan processes, sockets, filesystems, execute arbitrary shell commands, or access SQLite.
3. **React is presentation/state projection.** Runtime truth originates in Rust.
4. **Commands are narrow.** Expose intention-revealing Tauri commands such as `terminate_process(pid)` or `open_project_folder(project_id)`, never generic arbitrary shell execution.
5. **Runtime state stays in memory.** Do not persist 2-second CPU/RAM/process snapshots.
6. **Persistence is small and meaningful.** Persist discovered projects, overrides, settings, and bounded activity events.
7. **No false ownership.** Do not attribute global PostgreSQL, Redis, Docker, or other services to a project unless ownership is supported by cwd/process-tree evidence.
8. **No LLM/AI classification.** v0.1 discovery is deterministic, local-only, and explainable.
9. **Cross-platform code is explicit.** Keep macOS/Linux differences behind platform abstractions rather than spreading `cfg` branches throughout the codebase.
10. **YAGNI.** Do not implement features explicitly marked out of scope.

## Development workflow

- Start by inspecting the existing repository, package manifests, Tauri config, and generated source layout.
- Create an implementation plan from the supplied docs before coding.
- Implement in small vertical slices.
- Prefer TDD for deterministic engine logic: project detection, monorepo root selection, process-tree grouping, component classification, port association, primary URL selection, and persistence.
- Keep modules focused. Avoid premature enterprise/clean-architecture ceremony.
- Preserve compileability after each substantial slice.
- Do not silently weaken requirements to make tests pass.

## Verification gate

Before declaring the project complete, run all commands applicable to the finished repository and fix failures:

```bash
bun install
bun run build
bun test
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
bun run tauri build
```

If the package scripts differ after implementation, use the equivalent scripts actually present and document them in `README.md`.

`bun run tauri build` must be attempted on the current platform. Do not claim the other OS was runtime-verified unless it actually was; cross-platform code should still compile conditionally and be covered by unit tests where possible.

## Definition of done

Do not stop at a mock UI. Completion means the real Rust discovery engine is wired to the real React UI, persistence works, project/process actions are functional where permissions allow, tests pass, the application builds, and the acceptance criteria in `docs/quality/01-testing-and-acceptance.md` are satisfied.

When OS permissions prevent an operation, represent it honestly with a structured error or limited-access state instead of faking success.
