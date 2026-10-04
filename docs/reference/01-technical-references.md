# Technical References

These references are implementation aids, not replacements for the LocalDeck specification. APIs and versions can change; inspect the current repository/toolchain and current official docs during implementation.

## Tauri 2

IPC:
https://v2.tauri.app/concept/inter-process-communication/

Tray configuration:
https://v2.tauri.app/reference/config/

Autostart plugin:
https://v2.tauri.app/plugin/autostart/

Global shortcut plugin (optional v0.1 feature):
https://v2.tauri.app/plugin/global-shortcut/

## Rust system/process information

`sysinfo`:
https://docs.rs/sysinfo/latest/sysinfo/

As of the design date, current `sysinfo` exposes process information and supports macOS/Linux. Newer releases can have higher minimum Rust versions, so choose a version compatible with the project's actual Rust toolchain rather than blindly forcing latest.

## Rust socket information

`netstat2`:
https://docs.rs/netstat2/latest/netstat2/

It provides cross-platform socket information and associated PID data where supported, using lower-level OS mechanisms rather than requiring a continuously executed CLI command.

## SQLite

`rusqlite`:
https://docs.rs/rusqlite/latest/rusqlite/

SQLite belongs behind the Rust application layer in LocalDeck; do not use direct frontend SQL bindings for this design.

## Frontend

shadcn/ui:
https://ui.shadcn.com/

Zustand:
https://zustand.docs.pmnd.rs/

Tailwind CSS:
https://tailwindcss.com/docs

## Verification note

If an API signature differs from assumptions in the design docs, prefer the current official API while preserving the documented behavioral contract.
