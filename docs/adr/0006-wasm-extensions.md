# ADR 0006: WebAssembly extensions

- **Status:** Accepted
- **Date:** 2026-10-24

## Context

Milestone 6 asks for a WebAssembly extension MVP. A separately compiled extension must be able
to add functionality to the running editor. To do that it needs:

- commands;
- document read access and change notifications;
- validated edits;
- diagnostics;
- configuration and logging.

It must also respect permissions and resource limits. The design brief requires that extensions
not block typing, that misbehaving extensions not crash the editor, that the API be typed and
versioned (WIT and the Component Model where practical), and that a manifest declaration not be
permission by itself.

## Decisions

### Components with a WIT interface, run by Wasmtime

The API is one WIT package, `ferrite:extension@1.0.0`, in the `ferrite-extension-api` crate.

- **Host side.** `ferrite-extension-host` generates Wasmtime host bindings from it
  (`bindgen!`).
- **Extension side.** The same crate generates the Rust bindings extensions use (wit-bindgen,
  behind an `export_extension!` macro).
- **Single source.** Both sides compile against the same file, so a mismatch is a build error
  rather than a runtime surprise.
- **Version.** Wasmtime 50, with only the features needed: Cranelift, component model, runtime.

Alternatives considered:

- **A hand-rolled ABI over core WebAssembly** (pointers and lengths, JSON or bincode messages).
  It has fewer moving parts, but it is an untyped protocol that every extension language would
  need to reimplement. The brief asks for typed interfaces where practical, and they are
  practical.
- **Native plugins (dynamic libraries).** Ruled out by the brief: no sandbox at all.

### No WASI: modules built for `wasm32-unknown-unknown`, componentized on load

Extensions are built for `wasm32-unknown-unknown`. wit-bindgen embeds the interface description
in the module, and the host turns the module into a component when it loads it (`wit-component`;
loading the example, including this and compilation, takes 15–20 ms). The host links nothing but
the Ferrite API.

- **Imports.** An extension's only imports are the functions in `host`. There is no filesystem,
  network, environment, clock, or random source to forget to lock down.
- **Toolchain.** The target is a standard rustup target with no extra tooling.
- **Other inputs.** A ready-made component (from another language) loads too, if it imports
  only the Ferrite API.

Alternatives considered:

- **`wasm32-wasip2` with `wasmtime-wasi` and an empty context.** Most toolchains target this.
  But even with no preopened directories and no sockets, it brings in a large host surface
  (filesystem, sockets, clocks, random) that would have to be audited and kept empty, plus Tokio.
  Rust on `wasm32-unknown-unknown` loses nothing an extension needs: `std::fs` and `std::env`
  simply return errors. WASI can be added later behind a permission if a use case appears.

### One thread and one bounded queue per extension

Each running extension has a worker thread that owns its store and instance and processes events
one at a time.

- **Queueing.** The editor only ever pushes events: it holds the editor lock just long enough to
  queue a change, never to call into WebAssembly.
- **Merging.** Consecutive changes to a document merge while they wait. Past 1 MiB of changed
  text, or when events had to be dropped from a full queue (1,024 events), the extension is
  told to re-read the document.
- **Commands.** Commands are queued like events. The caller waits on a channel, off the UI
  thread.

Alternatives considered:

- **A shared pool with async Wasmtime.** It needs fewer threads with many extensions, but brings
  in an async runtime and fiber stacks. Threads are simple and isolate failures. With a handful
  of extensions the cost is a few idle threads. Revisit if extension counts grow.

### Deadlines by epoch interruption, memory by a resource limiter

A ticker thread advances the engine's epoch every 10 ms. Each store's epoch callback interrupts
the current call once it passes its deadline, or as soon as it is cancelled (on shutdown or
restart). The deadlines are 5 s for activation, 10 s per command, and 1 s per notification. This
bounds wall-clock time, including infinite loops, at negligible cost.

A `ResourceLimiter` caps:

- linear memory (128 MiB);
- table size;
- the numbers of instances, tables, and memories.

It records when the memory cap was hit, so the failure is reported as "exceeded its memory
limit" rather than as a generic crash. A trap, panic, stack overflow, or missed deadline ends the
instance. The extension is marked failed, and its diagnostics are removed.

Alternatives considered:

- **Fuel.** Deterministic, but it instruments every block (a measurable slowdown) and counts
  instructions rather than time. Wall-clock deadlines are what matter for responsiveness.
- **Restarting automatically after failures.** An extension that loops on every change would
  burn a second of CPU each time. Failures need the user's **Restart** instead, which also makes
  them visible.

### Validate everything at the API boundary

Host functions check permissions, then sizes, then document identity, version, and ranges,
before acting.

- **Positions.** Positions must be inside the document and on character boundaries.
- **Edits.** Edits must not overlap and must name the current version. They are applied through
  a new core method, `Document::apply_edits`: one validated `ChangeSet`, one undo step, with
  selections mapped through it.
- **Diagnostics.** Diagnostics may name a recently notified version (the last eight snapshots
  are kept); they are converted in that text and mapped through later edits.
- **Output.** Logs and messages are truncated and rate-limited.

Stale edits are rejected rather than rebased. Rebasing arbitrary edits over user typing can't be
made correct in general, and the extension can re-read and retry.

### Permissions are the user's decision

The manifest lists requested permissions (`documents_read`, `documents_write`). An extension
that requests permissions the user hasn't decided about doesn't start. The workbench asks with a
notification (Allow / Don't Allow / Details), and the Extensions view shows and changes each
grant.

Decisions persist in `extension-permissions.json`. A missing or invalid file grants nothing.
Effective permissions are the intersection of requested and granted, passed to `activate`, and
checked on every call. Changing them restarts the extension.

### Declarative contributions

Commands are declared in the manifest. The workbench lists them in the palette before the
extension runs, and running one activates the extension. This is the brief's host-managed,
declarative contribution model. Status bar items, views, and keybindings will follow the same
pattern (M7).

## Consequences

- **Startup.** Extensions cost nothing until they activate. Compiling and activating the example
  takes about 15–20 ms on its own thread. The editor's startup and idle memory are unchanged.
  With the example active, the app process uses about 11 MB more, and keystroke latency is
  unchanged.
- **Binary size.** The binary grows from 20.7 MB to 37.3 MB, mostly Cranelift. Caching compiled
  components, or an interpreter for small extensions, could be considered if this matters.
- **Restarts.** Extensions can't keep state across restarts and have no storage API yet.
- **Other languages.** Most other languages' toolchains produce WASI components, which this host
  rejects. Supporting them means providing a reviewed subset of WASI behind permissions.
- **Notifications.** An extension that publishes diagnostics on every change causes an extra view
  refresh per change for the active document. That stays within a frame, but it is IPC the
  editor wouldn't otherwise do.
