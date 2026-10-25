# Ferrite

A fast, memory-efficient, extensible code editor built with Rust, Tauri, React, and a sandboxed
WebAssembly extension system.

> **Status:** All eight milestones of the initial plan are complete. Milestone 8 (hardening)
> added recovery of unsaved changes after a crash, screen reader and high-contrast support,
> per-extension resource accounting, CI on Windows, macOS, and Linux, and performance regression
> checks. See [`docs/DESIGN.md`](docs/DESIGN.md) for the brief and milestone plan,
> [`docs/performance.md`](docs/performance.md) for measurements, and
> [`CONTRIBUTING.md`](CONTRIBUTING.md) for how to work on Ferrite.

## Prerequisites

| Tool                                                | Version used                                 |
| --------------------------------------------------- | -------------------------------------------- |
| [Rust](https://rustup.rs) (stable, MSVC on Windows) | 1.99.0                                       |
| [Node.js](https://nodejs.org)                       | 26.x                                         |
| [pnpm](https://pnpm.io)                             | 10.27.0 (pinned via `packageManager`)        |
| Platform dependencies for Tauri 2                   | see the [Tauri prerequisites][tauri-prereqs] |

On Windows this means the Visual Studio C++ build tools and the WebView2 runtime (preinstalled on
Windows 11). Linux needs WebKitGTK 4.1 development packages. Building extensions, and the
extension host's tests, also need `rustup target add wasm32-unknown-unknown`.

[tauri-prereqs]: https://v2.tauri.app/start/prerequisites/

## Getting started

```sh
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` starts the Vite dev server on port 1420, builds the Rust backend, and opens the
desktop window. Frontend changes hot-reload; Rust changes trigger a rebuild and restart.

Files passed on the command line open as tabs, for example
`target/debug/ferrite path/to/file.rs` (the debug binary needs `pnpm dev` running).

## Editing

Ferrite currently opens UTF-8 text files of up to 256 MiB. Line endings and a UTF-8 BOM are
preserved exactly, and new lines use the file's existing line ending.

| Action                              | Windows / Linux                  | macOS                    |
| ----------------------------------- | -------------------------------- | ------------------------ |
| Command palette                     | Ctrl+Shift+P, F1                 | ⇧⌘P, F1                  |
| Go to file / Go to line             | Ctrl+P / Ctrl+G                  | ⌘P / ⌃G                  |
| New file / Open / Close tab         | Ctrl+N / Ctrl+O / Ctrl+W         | ⌘N / ⌘O / ⌘W             |
| Save / Save As                      | Ctrl+S / Ctrl+Shift+S            | ⌘S / ⇧⌘S                 |
| Find / Replace                      | Ctrl+F / Ctrl+H                  | ⌘F / ⌥⌘F                 |
| Next / previous match               | F3 / Shift+F3 (Enter in find)    | F3 / ⇧F3                 |
| Add next occurrence                 | Ctrl+D                           | ⌘D                       |
| Find in files                       | Ctrl+Shift+F                     | ⇧⌘F                      |
| Extensions view                     | Ctrl+Shift+X                     | ⇧⌘X                      |
| Explorer / Toggle side bar          | Ctrl+Shift+E / Ctrl+B            | ⇧⌘E / ⌘B                 |
| Next / previous tab                 | Ctrl+Tab, Ctrl+PageDown / PageUp | ⌃Tab, ⌃PageDown / PageUp |
| Settings                            | Ctrl+,                           | ⌘,                       |
| Undo / Redo                         | Ctrl+Z / Ctrl+Y, Ctrl+Shift+Z    | ⌘Z / ⇧⌘Z                 |
| Cut / Copy / Paste / Select all     | Ctrl+X / C / V / A               | ⌘X / C / V / A           |
| Move by word                        | Ctrl+←/→                         | ⌥←/→                     |
| Line start (smart) / end            | Home / End                       | ⌘← / ⌘→                  |
| Document start / end                | Ctrl+Home / Ctrl+End             | ⌘↑ / ⌘↓                  |
| Delete word                         | Ctrl+Backspace / Ctrl+Delete     | ⌥Backspace / ⌥Delete     |
| Add cursor above / below / at click | Ctrl+Alt+↑ / ↓, Alt+click        | ⌘⌥↑ / ↓, ⌥click          |
| Select word / one cursor            | Double-click / Escape            | Double-click / Escape    |
| Go to definition                    | F12, Ctrl+click                  | F12, ⌘click              |
| Trigger suggestions                 | Ctrl+Space                       | ⌃Space                   |

A minimap beside the scrollbar shows the whole document small, in its syntax colors. Click it to
jump to that part of the file, or drag its slider to scroll. **View: Toggle Minimap** hides or
shows it. The editor scrolls past the last line until it's the only one visible, and clicking
below the text moves the cursor to the end of the last line.

In the find widget, Alt+C, Alt+W, and Alt+R toggle Match Case, Whole Word, and Regular
Expression; in the replace field, Enter replaces and Ctrl+Enter replaces all. Regex
replacements can use `$1` and `${name}`.

Hold Shift with any movement key to extend the selection. Closing a tab or the window with
unsaved changes asks whether to save them. If another program changes an open file, Ferrite
reloads it when you have no unsaved changes, and otherwise keeps your version and asks before
overwriting when you save.

## Languages

Rust, TypeScript (and TSX), JavaScript, JSON, TOML, and Python files are highlighted with
Tree-sitter grammars built into Ferrite.

With a language server installed, Ferrite also shows errors and warnings as you type (squiggles,
plus counts in the status bar), shows type information and documentation when the pointer rests
on a symbol, goes to definitions (F12 or Ctrl/⌘+click; holding Ctrl/⌘ underlines the symbol under
the pointer when it has one), and offers completions (as you type, after `.` and similar
characters, or with Ctrl+Space). Servers start when you open a file that needs one:

| Language                | Server                       | Install                                                |
| ----------------------- | ---------------------------- | ------------------------------------------------------ |
| Rust                    | `rust-analyzer`              | `rustup component add rust-analyzer`                   |
| TypeScript / JavaScript | `typescript-language-server` | `npm install -g typescript-language-server typescript` |
| Python                  | `pyright-langserver`         | `npm install -g pyright`                               |

The status bar shows the server's state; hover it for details if a server is missing or failed.
Servers can be changed, added, or turned off in the `[lsp]` section of the settings.

## Extensions

Extensions are WebAssembly modules that run in a sandbox: they can only use Ferrite's
[extension API](crates/ferrite-extension-api/wit/extension.wit). With it they add commands to
the palette, read open documents, edit them, and show problems as squiggles. They can also
customize the workbench: color themes, file icon themes, keybindings, context menu items, status
bar items, side bar views, and highlights in the text. They have no access to files, the network, or anything
else on your computer. Before an extension starts, Ferrite
asks you to allow the permissions it requests (reading or editing open documents). The
Extensions view (`Ctrl+Shift+X`) shows each extension's state and permissions, and lets you
change them, restart an extension, or turn it off. An extension that crashes, loops, or uses too
much memory is stopped without affecting the editor.

Four example extensions are included:

- [Tidy](extensions/tidy/) flags trailing whitespace and long lines, and can trim them.
- [TODOs](extensions/todos/) highlights TODO and FIXME comments and lists them in a side bar
  view. It shows the count in the status bar, jumps to the next one with `Ctrl+Alt+T`, and adds
  **Insert TODO Comment** to the editor's context menu.
- [Example Themes](extensions/themes/) adds the Midnight and Paper color themes, with no code.
- [Example File Icons](extensions/icons/) adds the Colorful file icon theme, with no code.

To try them:

```sh
rustup target add wasm32-unknown-unknown
cd extensions && cargo build --release
```

Then run **Extensions: Load Development Extension…** and choose an extension's folder, such as
`extensions/todos`. To install
an extension, copy its folder (manifest and `.wasm`) into the `extensions` folder next to
`settings.toml`. See [`docs/extensions.md`](docs/extensions.md) for writing your own.

## Customizing

- **Themes.** **Preferences: Color Theme: …** in the command palette switches between Dark, Light,
  Follow System, and themes from extensions. You can override any theme color in the settings.
- **File icons.** **Preferences: File Icon Theme: …** switches the file icons in the explorer
  and Go to File between the built-in ones and icon themes from extensions.
- **Keybindings.** Set keybindings in the settings by command id. **Preferences: Keyboard
  Shortcuts** lists every command with its id and keys. Your keybindings take precedence over the
  built-in ones, which take precedence over extensions'.

```toml
[workbench]
theme = "example.themes.midnight"
icon_theme = "example.icons.colorful"

[workbench.colors]
editor-background = "#101018"            # any token in src/themes/tokens.json

[keybindings]
"workbench.showSearch" = "mod+shift+h"   # "mod" is Ctrl, or Cmd on macOS
"example.todos.next" = ["f8", "ctrl+alt+t"]
"workbench.toggleSidebar" = ""           # remove a command's keys
```

## Recovering unsaved changes

Unsaved changes are backed up in the background while you work. If Ferrite crashes, is killed,
or the computer loses power, the next start reopens them as unsaved tabs and says so. Closing
Ferrite normally still asks about unsaved changes, and changes you choose not to save are not
brought back.

If part of the window shows "stopped working because of an internal error", choose **Try
Again**, or **Reload Window** (also **Developer: Reload Window** in the command palette).
Reloading keeps open documents and unsaved changes. See
[`docs/troubleshooting.md`](docs/troubleshooting.md) for more.

## Accessibility

- **Screen readers.** The editor exposes the cursor's line and the selection to screen readers,
  so they can read the line, character, word, or selection as you move. Errors and
  notifications are announced.
- **Keyboard.** Commands, views, lists, and menus work from the keyboard. Context menus open
  with `Shift+F10` or the menu key, and the side bar is resized with the arrow keys on its edge.
- **System settings.** Windows contrast themes (forced colors) and "reduce motion" are honored.
- **Fonts and colors.** The editor font is a setting (`editor.font_size`, `editor.font_family`),
  and any theme color can be overridden (see [Customizing](#customizing)).

## Workspace

Open a folder with **File: Open Folder…** (command palette or the Explorer). The explorer shows
`.gitignore`d entries dimmed.

To add a file or folder, use the **New File** and **New Folder** buttons in the Explorer header,
right-click a folder or file, or run **Explorer: New File…** / **Explorer: New Folder…**. The new
entry goes inside the selected folder, or beside the selected file. Right-clicking the empty space
below the files creates at the top level. Type a name and press
`Enter` (or `Escape` to cancel); a name like `src/net/http.rs` creates the missing folders too.
New files open in the editor. Go to File and find in files skip them, along with paths matching
`files.exclude`. The folder, open files, and layout are restored on the next launch.

## Settings

Run **Preferences: Open Settings (TOML)** (`Ctrl+,`) to edit `settings.toml`; saving applies it.
Every available setting is listed, commented out, in the file. Invalid settings are reported and
the previous ones stay in effect.

```toml
[editor]
tab_size = 2
font_size = 15

[workbench]
theme = "light"   # "system", "dark", "light", or an extension's theme id
icon_theme = "default"   # or an extension's file icon theme id

[files]
exclude = [".git", "dist"]

[lsp.servers.rust-analyzer]
command = "C:/tools/rust-analyzer.exe"   # use a specific build

[extensions.settings."example.tidy"]
max_line_length = 120                    # settings an extension declares
```

| Platform | Settings                                            | Session state                                       |
| -------- | --------------------------------------------------- | --------------------------------------------------- |
| Windows  | `%APPDATA%\dev.ferrite.editor\settings.toml`        | `%LOCALAPPDATA%\dev.ferrite.editor\state.json`      |
| macOS    | `~/Library/Application Support/dev.ferrite.editor/` | `~/Library/Application Support/dev.ferrite.editor/` |
| Linux    | `~/.config/dev.ferrite.editor/`                     | `~/.local/share/dev.ferrite.editor/`                |

## Commands

| Command                                                 | What it does                                                    |
| ------------------------------------------------------- | --------------------------------------------------------------- |
| `pnpm tauri dev`                                        | Run the desktop app in development mode                         |
| `pnpm tauri build`                                      | Build a release binary and platform installers                  |
| `pnpm tauri build --no-bundle`                          | Build the release binary only (`target/release/ferrite[.exe]`)  |
| `pnpm dev`                                              | Run only the frontend in a browser (no backend; IPC is stubbed) |
| `pnpm test`                                             | Run frontend tests (Vitest + Testing Library)                   |
| `pnpm typecheck`                                        | Type-check the frontend                                         |
| `pnpm format`                                           | Format frontend (Prettier) and Rust (`cargo fmt`) sources       |
| `pnpm format:check`                                     | Check formatting without writing                                |
| `pnpm bench`                                            | Criterion microbenchmarks (`benches/`)                          |
| `pnpm bench:app all`                                    | End-to-end startup, typing, and scrolling benchmarks (release)  |
| `pnpm bench:app typing --file F`                        | Typing benchmark, also typing into file `F` (e.g. a `.rs` file) |
| `pnpm bench:app typing --extensions`                    | Typing benchmark with your extensions loaded                    |
| `pnpm bench:check`                                      | Compare the last end-to-end run with `benches/baseline.json`    |
| `node scripts/perf-check.mjs micro`                     | Check Criterion's comparison with a saved baseline              |
| `cd extensions && cargo build --release`                | Build the example and test extensions to WebAssembly            |
| `cargo test --workspace`                                | Run Rust tests                                                  |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint Rust sources                                               |

## Logging

The backend logs through [`tracing`](https://docs.rs/tracing) to stderr and to a daily rolling
file (seven files retained) in the platform log directory:

| Platform | Log directory                            |
| -------- | ---------------------------------------- |
| Windows  | `%LOCALAPPDATA%\dev.ferrite.editor\logs` |
| macOS    | `~/Library/Logs/dev.ferrite.editor`      |
| Linux    | `~/.local/share/dev.ferrite.editor/logs` |

Set `FERRITE_LOG` to change the filter, using
[`EnvFilter` syntax](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html).
The default is `info`. Frontend messages are logged under the `frontend` target:

```sh
FERRITE_LOG=debug,frontend=trace pnpm tauri dev
```

Startup timing is logged as `backend initialized` and `frontend ready`, each with `elapsed_ms`
measured from the start of `ferrite_app::run`.

## Repository layout

```text
Cargo.toml            Cargo workspace
package.json          Frontend package and scripts
benches/              Criterion benchmarks and fixtures
scripts/              End-to-end benchmark harness and performance regression checks
.github/workflows/    CI: formatting, lints, and tests on Windows, macOS, and Linux
crates/
  ferrite-extension-api/   Extension API: WIT interface and Rust bindings for extensions
  ferrite-extension-host/  Extension loading, permissions, sandbox, limits, host functions
  ferrite-tasks/      Background scheduler and timing metrics
  ferrite-syntax/     Tree-sitter grammars, incremental parsing, highlighting
  ferrite-lsp/        Language server client: processes, JSON-RPC, positions, document sync
  ferrite-text/       Rope-backed text buffer, change sets, positions, Unicode boundaries
  ferrite-workspace/  Files, explorer listings, Go to File index, find in files, watching
  ferrite-core/       Documents, selections, editing operations, undo, dirty state
src-tauri/            ferrite-app crate: Tauri shell, lifecycle, IPC commands, dialogs, logging
src/                  React + TypeScript workbench
  app/                Root component, workbench layout, view registry
  commands/           Command registry, keybindings, fuzzy matching, built-in commands
  components/         Workbench chrome (activity bar, side bar, palette, notifications, ...)
  editor/             Editor view, find widget, hover and completion, tabs, keymap, session
  panels/             Side bar views (explorer, search, extensions)
  state/              Small observable stores (documents, workspace, search, layout, ...)
  ipc/                Typed wrappers for Tauri commands (the only place that calls invoke)
  themes/             Theme selection
  styles/             Design tokens and base styles
extensions/           Extensions built to WebAssembly: examples (tidy, todos, themes, icons) and test fixtures
docs/                 Architecture notes, ADRs, extension guide, troubleshooting
```

See [`docs/architecture.md`](docs/architecture.md) for how these pieces fit together and how the
layout will grow.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
