# Architecture

This document describes Ferrite's architecture as it currently exists and how it is expected to
grow. The goals and constraints come from [`DESIGN.md`](DESIGN.md); decisions with lasting
consequences are recorded as ADRs in [`adr/`](adr/).

## Current state (Milestone 1)

```text
┌──────────────────────── Desktop process (ferrite) ────────────────────────┐
│                                                                           │
│  ┌──────────── WebView ────────────┐      ┌──────── ferrite-app ────────┐ │
│  │ React workbench (src/)          │      │ lib.rs     lifecycle, wiring│ │
│  │   components → ipc/ wrappers ───┼─IPC─▶│ commands.rs  Tauri commands │ │
│  │                                 │      │ logging.rs   tracing setup  │ │
│  └─────────────────────────────────┘      └─────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────────┘
```

There is one Rust crate, `ferrite-app` (in `src-tauri/`). The design brief lists separate
`editor-app` and `src-tauri` components; in Ferrite they are the same crate, because the
application crate's job (lifecycle, dependency wiring, the command boundary) is exactly what the
Tauri shell does. Editor crates are created when they have real responsibilities (see
[ADR 0001](adr/0001-application-skeleton.md)).

## Layers and planned crates

Crates use the `ferrite-` prefix. Dependencies point downward only; no crate depends on Tauri or
React except `ferrite-app`.

| Layer (DESIGN.md §3.1)            | Home                                                  | Arrives in    |
| --------------------------------- | ----------------------------------------------------- | ------------- |
| Desktop lifecycle, OS integration | `ferrite-app` (`src-tauri/`)                          | M1 ✅         |
| Frontend and workbench UI         | `src/`                                                | M1 ✅ (shell) |
| Text storage and editing          | `ferrite-text`                                        | M2            |
| Editor core                       | `ferrite-core`                                        | M2            |
| Workspace and filesystem          | `ferrite-workspace`                                   | M3            |
| Language services                 | `ferrite-syntax`, `ferrite-lsp`                       | M5            |
| Extension API                     | `ferrite-extensions-api`                              | M6            |
| Extension runtime and security    | `ferrite-extension-host`                              | M6            |
| Settings and application state    | to be decided (likely `ferrite-core` + `ferrite-app`) | M3            |

## The frontend–backend boundary

- **Rust owns authoritative state.** In M1 the only state is process metadata. From M2, documents
  live in `ferrite-core`, and the frontend renders views of them.
- **Commands are the only entry point.** Every Tauri command is defined in
  `src-tauri/src/commands.rs` and mirrored by a typed wrapper in `src/ipc/index.ts`. Components
  never call `invoke` directly, so the boundary can be audited and changed in one place.
- **Payloads are explicit DTOs.** Command arguments and results are serializable structs with
  camelCase fields, defined for the boundary and not shared with internal types.
- **Inputs are validated in Rust.** For example, `frontend_log` rejects unknown levels and truncates
  messages over 8 KiB at a UTF-8 boundary.
- **Browser fallback.** Outside the desktop shell (`pnpm dev`, tests) the IPC wrappers return
  `null` or write to the console, so the workbench renders without a backend.

Current commands:

| Command          | Purpose                                                         |
| ---------------- | --------------------------------------------------------------- |
| `app_info`       | Product name, version, OS, architecture, Tauri version          |
| `frontend_log`   | Forward a frontend log message to `tracing` (target `frontend`) |
| `frontend_ready` | Signal first render; logs launch-to-interactive time once       |

### Permissions and CSP

`src-tauri/capabilities/default.json` grants the main window **no** Tauri core or plugin
permissions; Ferrite's own commands are always callable. Permissions are added only when a feature
needs them.

The production Content Security Policy allows only same-origin scripts, IPC, and inline styles.
Inline styles are needed for layout values set from React. The development CSP additionally allows
Vite's HMR socket and inline scripts (React Refresh).

## Frontend structure

- **`app/`**: `App` fetches backend data and reports readiness. `Workbench` owns the layout
  state (active view, side bar width) using React local state. No state library is warranted yet.
- **`app/views.ts`**: side bar views are declarative descriptors (`id`, `title`, `icon`,
  `component`). The activity bar and side bar render from this list. Extension-contributed views
  (M7) will feed the same shape, with host-rendered components instead of extension code.
- **Re-render boundaries**: `ActivityBar`, `EditorArea`, and `StatusBar` are memoized with stable
  props, so dragging the sash re-renders only the side bar and sash.
- **Theming**: `styles/tokens.css` defines semantic tokens (`--color-editor-background`,
  `--color-statusbar-background`, syntax colors, spacing, typography) for dark and light themes,
  selected by `data-theme` on `<html>`. Components use tokens only. `themes/theme.ts` follows the
  OS color scheme until a theme setting exists.
- **Accessibility**: activity bar items are toggle buttons (`aria-pressed`), the sash is a focusable
  `separator` adjustable with arrow keys, and editor tabs use the tab/tabpanel pattern.

## Logging and diagnostics

`ferrite-app` installs a `tracing` subscriber during Tauri's `setup` hook, once the platform log
directory is known:

- Output goes to stderr (colored) and a daily rolling file through a non-blocking writer. Seven
  files are retained.
- `FERRITE_LOG` overrides the default `info` filter. Invalid directives fall back to the default
  with a warning.
- If the log directory can't be created, logging continues on stderr only. Logging problems never
  prevent startup.
- Panics are logged through `tracing` before the default hook runs.
- The file writer is flushed on `RunEvent::Exit`, because Tauri may end the process without running
  destructors.
- Frontend uncaught errors and unhandled promise rejections are forwarded to the backend log.

Events emitted before `setup` runs (none from Ferrite code today) reach no subscriber.

## Startup measurement

`run()` records an `Instant` on entry. The backend logs `elapsed_ms` when setup finishes and when
the frontend first calls `frontend_ready` (after its first commit). These numbers give an early
baseline. Proper benchmarks come in M4.

Single, warm-cache runs on the development machine (Windows 11, x86_64), for orientation only:

| Build   | `backend initialized` | `frontend ready` |
| ------- | --------------------- | ---------------- |
| debug   | 490 ms                | 673 ms           |
| release | 332 ms                | 372 ms           |

The release `ferrite.exe` process had a working set of about 30 MB. WebView2 runs in separate
processes, which this figure excludes, so it isn't a total memory measurement.
