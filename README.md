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
| Problems / Output                   | Ctrl+Shift+M / Ctrl+Shift+U      | ⇧⌘M / ⇧⌘U                |
| Explorer / Toggle side bar          | Ctrl+Shift+E / Ctrl+B            | ⇧⌘E / ⌘B                 |
| Terminal / New / Toggle panel       | Ctrl+\` / Ctrl+Shift+\` / Ctrl+J | ⌃\` / ⌃⇧\` / ⌘J          |
| Next / previous tab (in its group)  | Ctrl+Tab, Ctrl+PageDown / PageUp | ⌃Tab, ⌃PageDown / PageUp |
| Next / previous group               | F6 / Shift+F6                    | F6 / ⇧F6                 |
| Settings                            | Ctrl+,                           | ⌘,                       |
| Undo / Redo                         | Ctrl+Z / Ctrl+Y, Ctrl+Shift+Z    | ⌘Z / ⇧⌘Z                 |
| Cut / Copy / Paste / Select all     | Ctrl+X / C / V / A               | ⌘X / C / V / A           |
| Move by word                        | Ctrl+←/→                         | ⌥←/→                     |
| Line start (smart) / end            | Home / End                       | ⌘← / ⌘→                  |
| Document start / end                | Ctrl+Home / Ctrl+End             | ⌘↑ / ⌘↓                  |
| Delete word                         | Ctrl+Backspace / Ctrl+Delete     | ⌥Backspace / ⌥Delete     |
| Insert line below / above           | Ctrl+Enter / Ctrl+Shift+Enter    | ⌘Enter / ⇧⌘Enter         |
| Move line up / down                 | Alt+↑ / ↓                        | ⌥↑ / ↓                   |
| Copy line up / down                 | Shift+Alt+↑ / ↓                  | ⌥⇧↑ / ↓                  |
| Delete line                         | Ctrl+Shift+K                     | ⇧⌘K                      |
| Indent / outdent line               | Ctrl+] / Ctrl+[, Tab / Shift+Tab | ⌘] / ⌘[, Tab / ⇧Tab      |
| Toggle line comment                 | Ctrl+/                           | ⌘/                       |
| Go to matching bracket              | Ctrl+Shift+\                     | ⇧⌘\                      |
| Fold / unfold                       | Ctrl+Shift+[ / Ctrl+Shift+]      | ⌥⌘[ / ⌥⌘]                |
| Toggle fold                         | Ctrl+K Ctrl+L                    | ⌘K ⌘L                    |
| Fold / unfold recursively           | Ctrl+K Ctrl+[ / Ctrl+K Ctrl+]    | ⌘K ⌘[ / ⌘K ⌘]            |
| Fold all / unfold all               | Ctrl+K Ctrl+0 / Ctrl+K Ctrl+J    | ⌘K ⌘0 / ⌘K ⌘J            |
| Fold all / unfold all regions       | Ctrl+K Ctrl+8 / Ctrl+K Ctrl+9    | ⌘K ⌘8 / ⌘K ⌘9            |
| Fold level 1 to 7                   | Ctrl+K Ctrl+1 … Ctrl+K Ctrl+7    | ⌘K ⌘1 … ⌘K ⌘7            |
| Add cursor above / below / at click | Ctrl+Alt+↑ / ↓, Alt+click        | ⌘⌥↑ / ↓, ⌥click          |
| Select word / one cursor            | Double-click / Escape            | Double-click / Escape    |
| Select line / all occurrences       | Ctrl+L / Ctrl+Shift+L            | ⌘L / ⇧⌘L                 |
| Cursors at line ends / cursor undo  | Shift+Alt+I / Ctrl+U             | ⌥⇧I / ⌘U                 |
| Scroll a line up / down             | Ctrl+↑ / ↓                       | ⌃↑ / ↓                   |
| Go to definition                    | F12, Ctrl+click                  | F12, ⌘click              |
| Trigger suggestions                 | Ctrl+Space                       | ⌃Space                   |

With nothing selected, cut and copy take the whole line, and pasting it inserts it as a line
above the cursor, as in VS Code. Tab indents when the selection spans lines. Toggle Line Comment
uses the comment syntax the language's extension declares; languages without line comments (HTML,
CSS) get a block comment around the lines.

Code folds where the language's fold query says (blocks, bodies, long argument lists, comments),
or by indentation for languages without one. Fold markers appear in the gutter when the pointer
is over it; click one to fold, or click the ⋯ after a folded line to unfold. A folded region
moves, copies, and deletes with its first line, and searching or undoing into it unfolds it.
Ferrite remembers what was folded in each file.

Region comments mark foldable regions of your own, written with the language's comment syntax:

```rust
// #region Helpers
fn a() {}
// #endregion
```

(`# region` in Python and TOML, `/* #region */` in CSS, `<!-- #region -->` in HTML.) Folding one
hides it through its `#endregion` line. The minimap labels named regions, and `// MARK: Name`
comments too (`// MARK: - Name` adds a separator line). `editor.minimap_region_headers` and
`editor.minimap_mark_headers` turn the labels off.

A minimap beside the scrollbar shows the whole document small, in its syntax colors. Click it to
jump to that part of the file, or drag its slider to scroll. **View: Toggle Minimap** hides or
shows it. The editor scrolls past the last line until it's the only one visible, and clicking
below the text moves the cursor to the end of the last line.

Breadcrumbs above the editor show where the file is, such as `src › components › Palette.tsx`.
Click a part to show it in the Explorer.

In the find widget, Alt+C, Alt+W, and Alt+R toggle Match Case, Whole Word, and Regular
Expression; in the replace field, Enter replaces and Ctrl+Enter replaces all. Regex
replacements can use `$1` and `${name}`.

Hold Shift with any movement key to extend the selection. Closing a tab or the window with
unsaved changes asks whether to save them. If another program changes an open file, Ferrite
reloads it when you have no unsaved changes, and otherwise keeps your version and asks before
overwriting when you save.

## Languages

Ferrite has no languages built in: each comes from an extension, so you add exactly the ones you
edit. Without any, files are plain text. A language extension brings a Tree-sitter grammar
compiled to WebAssembly for highlighting, and usually declares a language server. These are
included, with no code to build:

| Extension                            | Languages                | Language server                   | Install the server with                                |
| ------------------------------------ | ------------------------ | --------------------------------- | ------------------------------------------------------ |
| [Rust](extensions/rust/)             | Rust                     | `rust-analyzer`                   | `rustup component add rust-analyzer`                   |
| [TypeScript](extensions/typescript/) | TypeScript, TSX          | `typescript-language-server`      | `npm install -g typescript-language-server typescript` |
| [JavaScript](extensions/javascript/) | JavaScript, JSX          | `typescript-language-server`      | as above                                               |
| [JSON](extensions/json/)             | JSON                     | —                                 |                                                        |
| [TOML](extensions/toml/)             | TOML, `Cargo.lock`       | —                                 |                                                        |
| [Python](extensions/python/)         | Python                   | `pyright-langserver`              | `npm install -g pyright`                               |
| [HTML & CSS](extensions/html-css/)   | HTML (with CSS, JS), CSS | `vscode-html/css-language-server` | `npm install -g vscode-langservers-extracted`          |
| [Markdown](extensions/markdown/)     | Markdown                 | —                                 |                                                        |

Load one with **Extensions: Load Development Extension…** (choose its folder, such as
`extensions/rust`), or install it by copying its folder into the `extensions` folder next to
`settings.toml`. Ferrite asks before running an extension's language server; highlighting
works either way. Markdown's highlighting needs no build either; its preview does (see
[Extensions](#extensions)).

With a language server, Ferrite also shows errors and warnings as you type (squiggles, plus
counts in the status bar), shows type information and documentation when the pointer rests on a
symbol, goes to definitions (F12 or Ctrl/⌘+click; holding Ctrl/⌘ underlines the symbol under the
pointer when it has one), and offers completions (as you type, after `.` and similar
characters, or with Ctrl+Space). Servers start when you open a file that needs one. The status
bar shows the server's state; hover it for details if a server is missing or failed. Servers can
be changed, added, or turned off in the `[lsp]` section of the settings.

## Extensions

Extensions are WebAssembly modules that run in a sandbox: they can only use Ferrite's
[extension API](crates/ferrite-extension-api/wit/extension.wit). With it they add commands to
the palette, read open documents, edit them, and show problems as squiggles. They can also
customize the workbench: color themes, file icon themes, keybindings, context menu items, status
bar items, side bar views, highlights in the text, and previews (formatted pages, such as rendered
Markdown), and add languages. They have no access to
files, the network, or anything else on your computer. Before an extension starts, Ferrite asks
you to allow the permissions it requests (reading or editing open documents, or running the
language servers it declares). The
Extensions view (`Ctrl+Shift+X`) shows each extension's state and permissions, and lets you
change them, restart an extension, or turn it off. An extension that crashes, loops, or uses too
much memory is stopped without affecting the editor.

Besides the [language extensions](#languages), six example extensions are included:

- [Tidy](extensions/tidy/) flags trailing whitespace and long lines, and can trim them.
- [TODOs](extensions/todos/) highlights TODO and FIXME comments and lists them in a side bar
  view. It shows the count in the status bar, jumps to the next one with `Ctrl+Alt+T`, and adds
  **Insert TODO Comment** to the editor's context menu.
- [Example Themes](extensions/themes/) adds the Midnight and Paper color themes, with no code.
- [Example File Icons](extensions/icons/) adds the Colorful file icon theme, with no code.
- [HTML & CSS](extensions/html-css/) adds HTML and CSS highlighting and their language servers,
  with no code.
- [Markdown](extensions/markdown/) adds Markdown highlighting, and **Markdown: Open Preview**
  (`Ctrl+Shift+V`) and **Open Preview to the Side** (`Ctrl+K V`), which show the page rendered and
  update it as you type.

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
  with `Shift+F10` or the menu key. Groups are resized with the arrow keys on their edges, `F6`
  moves between them, and every docking action has a command (see [Layout](#layout)).
  `` Ctrl+` `` moves between the editor and the terminal.
- **Terminal output.** Set `terminal.screen_reader = true` to have screen readers announce
  terminal output.
- **System settings.** Windows contrast themes (forced colors) and "reduce motion" are honored.
- **Fonts and colors.** The editor font is a setting (`editor.font_size`, `editor.font_family`),
  and any theme color can be overridden (see [Customizing](#customizing)).

## Workspace

Open a folder with **File: Open Folder…** (command palette or the Explorer). The explorer shows
`.gitignore`d entries dimmed. It follows the file you're editing: switching tabs, going to a
definition, or opening a search result expands the file's folders and selects it.

To add a file or folder, use the **New File** and **New Folder** buttons in the Explorer header,
right-click a folder or file, or run **Explorer: New File…** / **Explorer: New Folder…**. The new
entry goes inside the selected folder, or beside the selected file. Right-clicking the empty space
below the files creates at the top level. Type a name and press
`Enter` (or `Escape` to cancel); a name like `src/net/http.rs` creates the missing folders too.
New files open in the editor. Go to File and find in files skip them, along with paths matching
`files.exclude`. The folder, open files, and layout are restored on the next launch.

## Layout

Everything is a tab: documents, the explorer, search, extensions, extension views, and terminals.
Tabs sit in groups, and any tab can go in any group, beside any other.

- **Docking.** Drag a tab (or an activity bar button) onto another group's tabs to join it, onto
  a side of a group to split it, or near an edge of the window to dock along that edge. A
  highlight shows where it will land.
- **From the keyboard.** The tab menu (right-click, `Shift+F10`, or the menu key) splits or moves
  a tab. The same actions are commands: **View: Split Tab Right/Down**, **View: Move Tab
  Left/Right/Up/Down** (into the neighbouring group, or a new one), **View: Close Tab**, and
  **View: Reset Layout**. In a tab strip, the arrow keys move between tabs and `Delete` closes one.
- **The side bar** is whatever is docked along the left edge, beside the activity bar. The
  activity bar has a button for each tab there (views, terminals, documents), and for views that
  aren't open; something docked elsewhere has no button. Drop a tab on the activity bar to move
  it into the side bar.
- **Hiding.** An activity bar button shows its tab, or hides its group when it's already
  showing. **View: Toggle Side Bar** (`Ctrl+B`) hides or shows the side bar, and **View: Toggle
  Panel** (`Ctrl+J`) the groups along the bottom (terminals, Problems, Output), keeping their
  places.
- **Problems and Output** open along the bottom, with the terminals. **Problems**
  (`Ctrl+Shift+M`, or the counts in the status bar) lists what's wrong with Ferrite and its
  extensions: an extension that stopped, settings that were ignored, and the warnings and errors
  notifications showed. Notifications disappear after a while, so this is where to find them
  again. **Output** (`Ctrl+Shift+U`) shows recent log messages from Ferrite, from each extension,
  and from each language server (including what the server prints), with **Open Log File** for
  the full log. Each extension in the Extensions view has a **Log** button.
- **Documents** open in the group last used for documents. The main group (on the right at
  first) stays when empty and shows the welcome page.

The layout is restored on the next launch, with documents found again by path.

## Terminal

**View: Toggle Terminal** (`` Ctrl+` ``) opens a terminal, docked along the bottom at first, in
the workspace folder; pressed in the terminal, it returns to the editor. **Terminal: New
Terminal** (`` Ctrl+Shift+` ``) or the **+** button beside a terminal's tabs opens another. Like
any tab, a terminal can be docked anywhere.

- **Shells.** Ferrite finds the shells installed on your computer:
  - **Windows:** PowerShell 7, Windows PowerShell, Command Prompt, Git Bash, Cygwin, MSYS2, and
    each WSL distribution, such as "Ubuntu (WSL)".
  - **macOS and Linux:** the shells listed in `/etc/shells`, such as bash, zsh, and fish.

  The arrow beside the **+** button, or **Terminal: New Terminal With Profile…**, starts a
  terminal with any of them. New terminals otherwise use the default profile: PowerShell on
  Windows (PowerShell 7 if installed), `$SHELL` elsewhere. Change it with **Terminal: Select
  Default Profile…**, or `terminal.default_profile`. In settings, you can also add profiles,
  change a found one's arguments or environment, or hide one (see [Settings](#settings)).

- **Keys.** While the terminal has focus, keys go to the shell, except the command palette, Go
  to File, showing views, switching tabs and groups, settings, and terminal commands.
- **Clipboard.** On Windows, `Ctrl+C` copies when text is selected and `Ctrl+V` pastes; on Linux
  use `Ctrl+Shift+C` and `Ctrl+Shift+V`; on macOS, `Cmd+C` and `Cmd+V`.
- **Lifetime.** `exit` closes the tab. A shell that fails leaves its tab open until you press a
  key. Terminals keep running when hidden or when the window reloads, and end when Ferrite
  quits.
- **On Windows**, Ferrite uses the console host from Windows Terminal, which it installs beside
  `ferrite.exe`, rather than the one built into Windows, which loses or repeats lines when a
  terminal is resized. The first Windows build downloads it (see
  [ADR 0011](docs/adr/0011-integrated-terminal.md#resizing-on-windows-2026-10-30)).
- **Colors.** The ANSI colors are theme tokens (`terminal-ansi-red`, and so on), so themes and
  `workbench.colors` can change them.

## Settings

Run **Preferences: Open Settings** (`Ctrl+,`) to see every setting, including those your
extensions declare, with its value and description. Changes are written to `settings.toml` and
apply at once. Search by name, key, or description; `@modified` shows what you've set, and
`@ext:<id>` shows one extension's settings (each extension's **Settings** button in the
Extensions view opens them).

**Preferences: Open Settings (TOML)** opens the file itself; saving applies it. Theme color
overrides, language servers, terminal profiles, and keybindings are edited there. Every available
setting is listed, commented out, in a new file. Invalid settings are reported and the previous
ones stay in effect.

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

[terminal]
default_profile = "Ubuntu (WSL)"         # a profile's name; empty: PowerShell, or $SHELL
scrollback = 5000
screen_reader = true                     # announce terminal output (slower)

[terminal.profiles."Nushell"]
command = "C:/tools/nu.exe"              # add a profile
args = []

[terminal.profiles."Command Prompt"]
enabled = false                          # hide one that was found

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

**View: Output** (`Ctrl+Shift+U`) shows recent messages without opening the file: Ferrite's,
each extension's, and each language server's, in separate logs. Those keep every level from
extensions and servers, including their debug messages and a server's stderr, whatever the
filter.

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
  ferrite-terminal/   Shells in pseudoterminals, output streaming, flow control
  ferrite-syntax/     Tree-sitter grammars, incremental parsing, highlighting
  ferrite-lsp/        Language server client: processes, JSON-RPC, positions, document sync
  ferrite-text/       Rope-backed text buffer, change sets, positions, Unicode boundaries
  ferrite-workspace/  Files, explorer listings, Go to File index, find in files, watching
  ferrite-core/       Documents, selections, editing operations, undo, dirty state
src-tauri/            ferrite-app crate: Tauri shell, lifecycle, IPC commands, dialogs, logging
src/                  React + TypeScript workbench
  app/                Root component, workbench layout, view registry
  commands/           Command registry, keybindings, fuzzy matching, built-in commands
  components/         Workbench chrome (activity bar, palette, notifications, ...)
  dock/               The layout: groups of tabs in splits, drag and drop, saving
  editor/             Editor view, find widget, hover and completion, tabs, keymap, session
  panels/             Views (explorer, search, extensions)
  terminal/           Terminal panel, tabs, and xterm.js sessions
  state/              Small observable stores (documents, workspace, search, layout, ...)
  ipc/                Typed wrappers for Tauri commands (the only place that calls invoke)
  themes/             Theme selection
  styles/             Design tokens and base styles
extensions/           Extensions built to WebAssembly: examples (tidy, todos, markdown, themes, icons) and test fixtures
docs/                 Architecture notes, ADRs, extension guide, troubleshooting
```

See [`docs/architecture.md`](docs/architecture.md) for how these pieces fit together and how the
layout will grow.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
