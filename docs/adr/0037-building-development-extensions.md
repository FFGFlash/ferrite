# ADR 0037: Building development extensions

- **Status:** Accepted
- **Date:** 2026-11-06

## Context

Working on an extension meant building it in a terminal, then clicking **Restart** (or **Reload
Extensions**) in Flareite, for every change. The user asked for an optional build script in
`extension.toml`. For development extensions, Flareite would watch the extension's folder and
rebuild the extension whenever something changes.

## Decisions

### `[build]`: a program and its arguments

```toml
[build]
command = "cargo"
args = ["build", "--release", "--package", "tidy"]
watch = ["src", "Cargo.toml"]   # optional
```

The command is a program plus its arguments, not a shell command line. `language_servers` and
`programs` work the same way, and so the manifest reads the same on Windows, macOS, and Linux:
there's no choice between `cmd` and `sh` and no quoting to get right on each. A script is
`command = "sh"` (or `pwsh`) with the script as an argument. As with language servers, a
command without a `/` or `\` is found on the `PATH` (with `.cmd` and `.bat` on Windows, for npm),
and one with a separator is relative to the extension's folder. It runs there, with Flareite's
environment, stdin closed, and no console window, in a process group (`flareite-process`), so
stopping it stops everything it started.

### Only development extensions build

The manifest is the same file when the extension is published, so an installed extension may
carry its `[build]`. It's validated like the rest but never run. The user put a development
extension's folder in `extensions.development` themselves, so it's their own code running.
Installed code never gets to start a program this way. `HostOptions::builds` turns builds off for
the benchmarks and tests.

### Read even when the rest is invalid

A fresh checkout has no `.wasm` yet, and a missing entry point makes the manifest invalid. So
`read_build_plan` reads only the `[build]` table, plus `id`, `name`, and `entrypoint` if they're
there, from the TOML, whatever else is wrong. The first build then makes the extension valid.
When `extension.toml` can't be parsed at all (half edited), an existing builder keeps watching, so
that saving the fix starts a build.

### What starts a build

Every file in the folder counts, except:

- anything in `.git`;
- files a `.gitignore` ignores: the folder's own, and its parents' up to the repository root;
- the entry point itself;
- changes reported for folders, which say nothing about which file changed and may be the build
  writing its output.

Without these rules a build's own output (`target/`, `dist/`) would start the next build. `watch`
narrows this further, using the `files.exclude` pattern rules: a name at any depth, or a path from
the folder. `.gitignore` files are read when watching starts, which is when the extension is
loaded or its `[build]` changes.

Changes are debounced (250 ms, the workspace watcher's), and paths are matched against both the
folder as configured and its canonical path, since macOS reports changes under the real path.

### One build at a time, and the extra build is coalesced

Each folder's builder thread runs one build at a time. Changes made during a build ask for one
more build after it, however many there are. They don't kill the running build: a partial build
is never loaded anyway (only success loads), and killing a build halfway through leaves some
tools in a worse state than letting it finish. Builds have no time limit, since some take minutes.
The cost is that a build command which never exits (a watch mode) blocks every later build. The
documentation says so.

### Loading the result reloads one extension

After a successful build, `reload_development` reads the manifest again. When the extension is
loaded with the same id, its entry is replaced in place, keeping its usage. The extension is
restarted if it was running (or had failed) and started if its activation wants it. Other
extensions keep running, unlike **Reload Extensions**. A manifest that becomes valid or invalid,
or changes its id, goes through the full `load()`, which handles id clashes. When the manifest
is still invalid, only its error is updated.

Some builds are no-ops: the one that runs when watching starts (at startup) usually is, for an
extension built earlier. Such a build reloads the extension only if the entry point's modification
time or size changed, or the extension isn't loaded. Otherwise every startup would restart every
development extension right after starting it. A build started by a change or by **Build** always
reloads, since the change may be the manifest, a theme, or a webview page, not code.

A builder re-reads `[build]` before each build. When it has changed, the extension is reloaded,
which replaces the builder with one for the new plan. Editing the build command takes effect on
the very next build.

### Where it shows

- **Output.** The build's output, line by line, goes to the extension's log channel at `debug`
  level. It shows in Output but not in the log file, so a chatty `cargo` doesn't fill the file.
  The command line, the outcome, and how long the build took are logged at `info` and `warn`.
- **Extensions view.** A build that's running or has failed shows on the extension's row. The
  page shows how the last build went, with **Build** and **Show Build Log**. Both also appear for
  an invalid extension, whose other actions are hidden. `ExtensionStatus::build` carries the
  build's state, error, duration, and log channel (the manifest's id, or the folder when there is
  none).
- **Notifications.** A failed build is announced once, as an error message from the extension
  (which also lists it in Problems). Later failures stay quiet until a build succeeds, because
  saving a file with a typo shouldn't toast on every save. The last successful build stays
  loaded.
- `Notification::BuildChanged` maps to the frontend's `extensionsChanged`, without the language
  refresh that a real change of extensions needs.

## Consequences

- A save in a development extension's folder runs its build. For Rust, `cargo build` is
  incremental, but a build still takes a moment, and a slow build delays seeing the change.
- Builds run Flareite's environment. When Flareite runs under `cargo run`, the `CARGO_*`
  variables cargo sets for a run are passed on, and none of them redirect a build.
- The examples Tidy and TODOs declare their builds. The official extensions in their own
  repositories can add one the same way (`cargo build --release`). Older versions of Flareite
  validate manifests with `deny_unknown_fields` and reject `[build]`. A released extension that
  keeps it can only be installed in versions that have it, and `engines.flareite` can't say so
  while the editor's version is still 0.1.0. So the official extensions should wait for a
  version bump, or leave it out of their packages.
- Not done: per-platform commands, a build timeout, cancelling a build from the UI, and watching
  development extensions that have no `[build]` (to reload on manifest or asset changes).
