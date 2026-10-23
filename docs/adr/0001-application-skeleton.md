# ADR 0001: Application skeleton

- **Status:** Accepted
- **Date:** 2026-10-23

## Context

Milestone 1 needs a working Tauri + React + TypeScript application with a Cargo workspace, logging,
and a basic workbench. Several choices made now are expensive to change later: the framework
version, the crate layout, the naming, and the IPC conventions.

## Decisions

### Tauri 2 (stable), not Tauri 3

Tauri 3 is in alpha (`3.0.0-alpha.5` at the time of writing). We use Tauri **2.12** with
`@tauri-apps/api` 2.12 and `@tauri-apps/cli` 2.12, keeping the npm and crate minor versions
aligned. We'll revisit when Tauri 3 is stable and its migration path is documented.

### Product name and crate naming

The product is **Ferrite** (bundle identifier `dev.ferrite.editor`). Crates use the `ferrite-`
prefix instead of the brief's `editor-` placeholder, for example `ferrite-text` and
`ferrite-core`.

### One crate for now; `src-tauri` is the application crate

The brief lists both `editor-app` and `src-tauri`. Both have the same responsibility (lifecycle,
dependency wiring, the command boundary), so they are one crate: `ferrite-app` in `src-tauri/`.
Following the brief's guidance not to create empty crates, the workspace currently has a single
member. `ferrite-text` and `ferrite-core` will be added in Milestone 2 under `crates/`.

`ferrite-app` exposes a library (`ferrite_app::run`) plus a thin `ferrite` binary. That keeps the
code unit-testable and leaves room for mobile entry points later.

### Workspace-managed dependencies

All Rust dependency versions live in the root `[workspace.dependencies]`. Member crates inherit
them. The Tauri CLI can't rewrite inherited feature lists, so Tauri features must be edited in the
root `Cargo.toml` by hand. The CLI prints an informational notice about this on each run.

Workspace lints deny `unsafe_code` and warn on `unwrap`, `todo!`, and `dbg!`.

### Logging with `tracing`

`tracing` + `tracing-subscriber` (`EnvFilter`, `fmt`) + `tracing-appender` (daily rolling file,
non-blocking). We chose this over `tauri-plugin-log`, which is built on the `log` crate. The brief
specifies `tracing`, and structured fields (`elapsed_ms`, `version`) matter for diagnostics.
Because the log directory is only known inside Tauri's `setup` hook, the subscriber is installed
there.

### Frontend toolchain

pnpm 10, Vite 8, React 19, TypeScript 7 (strict, with `noUncheckedIndexedAccess`), Vitest 5 with
jsdom and Testing Library, and Prettier. ESLint is deferred until there is enough code for lint
rules to pay for their configuration.

### State management

React local state and props. The workbench has two pieces of layout state. A state library would
add weight without benefit. We'll revisit when document and view state arrive (M2–M3).

### IPC conventions

All commands live in `commands.rs` and are mirrored by typed wrappers in `src/ipc/`. Payloads are
camelCase DTOs. Rust validates every input. See [architecture.md](../architecture.md).

### Least-privilege defaults

The main window's capability grants no Tauri core permissions, and the production CSP allows only
same-origin resources and IPC.

## Consequences

- The workspace is minimal and compiles quickly after the first build. Adding crates is cheap.
- Upgrading to Tauri 3 later will be a deliberate migration.
- Startup timing is logged from day one, which gives M4's benchmarks a reference point.
