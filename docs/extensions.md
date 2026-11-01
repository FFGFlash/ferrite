# Writing extensions

Ferrite extensions are WebAssembly components. They run in a sandbox inside the editor and can
do only what the [extension API](../crates/ferrite-extension-api/wit/extension.wit) offers:

- register commands;
- read open documents and get notified of changes;
- edit documents through validated, versioned edits;
- publish diagnostics (squiggles and hover messages);
- customize the workbench: color themes, keybindings, context menu items, status bar items,
  side bar views, and editor decorations;
- add languages: syntax highlighting from Tree-sitter grammars, and language servers;
- read their own settings;
- log, and show messages.

There is no filesystem, network, environment, clock, or process access. A manifest declares
the permissions an extension needs, and the user grants them before the extension starts.
(Language servers an extension declares are started by Ferrite, not by the extension, and only
once the user allows it.)

This guide uses these examples:

- [Tidy](../extensions/tidy/) flags trailing whitespace and long lines, and has commands to trim
  trailing whitespace and to count words.
- [TODOs](../extensions/todos/) uses every workbench contribution: it highlights TODO comments,
  lists them in a side bar view, counts them in the status bar, and adds a keybinding and a
  context menu item.
- [Example Themes](../extensions/themes/) contributes two color themes and no code at all.
- [HTML & CSS](../extensions/html-css/) adds two languages, highlights CSS and JavaScript inside
  HTML, and declares their language servers, also with no code. Ferrite has no languages of its
  own: [Rust](../extensions/rust/), [TypeScript](../extensions/typescript/),
  [JavaScript](../extensions/javascript/), [JSON](../extensions/json/),
  [TOML](../extensions/toml/), and [Python](../extensions/python/) are extensions like it.

## Quick start

You need Rust with the `wasm32-unknown-unknown` target:

```sh
rustup target add wasm32-unknown-unknown
cd extensions
cargo build --release          # builds every extension in the workspace
```

To load the result in Ferrite:

1. Run **Extensions: Load Development Extension…** from the command palette.
2. Choose `extensions/tidy`. This adds the folder to `extensions.development` in your settings.
3. Ferrite asks whether to allow the permissions Tidy requests.
4. Open a file with trailing spaces: they are underlined, and **Tidy: Trim Trailing Whitespace**
   removes them.

After rebuilding an extension, click **Restart** in the Extensions view (`Ctrl+Shift+X`) to load
the new build. To pick up new or removed extensions, use **Extensions: Reload Extensions**.
There's no need to rebuild or restart Ferrite.

## Anatomy of an extension

```text
my-extension/
  extension.toml          the manifest
  Cargo.toml              crate-type = ["cdylib"], depends on ferrite-extension-api
  src/lib.rs              implements `Guest` and calls `export_extension!`
```

```rust
use ferrite_extension_api::types::*;
use ferrite_extension_api::{Guest, export_extension, host};

struct Hello;

impl Guest for Hello {
  fn activate(_context: Activation) -> Result<(), String> {
    Ok(())
  }
  fn deactivate() {}
  fn run_command(_command: String, _context: CommandContext) -> Result<(), String> {
    host::show_message(MessageLevel::Info, "Hello from an extension!");
    Ok(())
  }
  fn document_opened(_document: DocumentInfo) {}
  fn document_changed(_change: DocumentChange) {}
  fn document_closed(_document: DocumentId) {}
}

export_extension!(Hello);
```

Build it as a `cdylib` for `wasm32-unknown-unknown`. The output is a core module that already
carries its interface description, embedded by wit-bindgen. Ferrite turns it into a component
when it loads it, so you need no other tools.

## The manifest

```toml
id = "example.tidy"               # publisher.name: lowercase letters, digits, '-'
name = "Tidy"
version = "0.1.0"                 # semantic version
description = "Flags trailing whitespace and overlong lines."
api_version = "1"                 # the extension API version it was built for
entrypoint = "tidy.wasm"          # relative to this file
activation = ["on-startup"]       # see below; default: when one of its commands runs

[engines]
ferrite = ">=0.1.0"               # optional semver requirement on the editor

[permissions]
documents_read = true
documents_write = true

[[commands]]
id = "example.tidy.trimTrailingWhitespace"   # must start with the extension id
title = "Trim Trailing Whitespace"
category = "Tidy"                 # optional; the extension's name by default

[configuration.max_line_length]
type = "integer"                  # boolean, integer, number, or string
default = 100
minimum = 0                       # numbers: optional minimum and maximum
description = "Lines longer than this are flagged; 0 turns the check off."

[configuration.severity]
type = "string"
default = "warning"
choices = ["error", "warning", "information", "hint"]   # strings: optional
```

Ferrite validates a manifest completely before loading anything. Any of these problems marks the
extension invalid in the Extensions view, with a message:

- unknown fields;
- an `api_version` other than `1`, or an unmet `engines.ferrite`;
- malformed ids, or command ids outside the extension's namespace;
- an activation event naming an undeclared command;
- a configuration default of the wrong type.

**Activation events:**

| Event              | When the extension starts                         |
| ------------------ | ------------------------------------------------- |
| `on-startup`       | When Ferrite starts (or the extension is loaded). |
| `on-language:<id>` | When a document of that language is opened.       |
| `on-command:<id>`  | When that command runs.                           |
| `on-view:<id>`     | When one of its side bar views is shown.          |

Every extension also starts when one of its commands runs, so `on-command` is only
documentation. Language ids are those extensions add (see [Languages](#languages)), such as
`rust`, `typescript`, `typescriptreact`, `javascript`, `json`, `toml`, `python`, `html`, and
`css` in the examples.

**Entry point.** An installed extension's entry point must be inside its folder. A development
extension may point elsewhere, such as Tidy's
`../target/wasm32-unknown-unknown/release/tidy.wasm`. The file can be at most 32 MiB.

**Hidden commands.** A command with `hidden = true` is left out of the command palette, for
commands run only from view items or other contributions.

**Dependencies** between extensions aren't supported in API version 1.

## Permissions

| Manifest key       | Allows                                                                           |
| ------------------ | -------------------------------------------------------------------------------- |
| `documents_read`   | Listing and reading open documents, change notifications, publishing diagnostics |
| `documents_write`  | Editing open documents                                                           |
| `language_servers` | Ferrite running the extension's [language servers](#language-servers)            |

Commands, settings, logging, messages, and languages need no permission.

**Requesting a permission doesn't grant it.** When an extension requests permissions the user
hasn't decided about, Ferrite asks before starting it. The user can allow or deny each
permission, now or later in the Extensions view. Decisions are stored in
`extension-permissions.json` next to `settings.toml`.

An extension with denied permissions still runs. Calls that need them fail with
`permission-denied`, and it gets no document notifications without `documents_read`.
`activate` receives the granted permissions, so the extension can adapt. Changing permissions
restarts the extension.

## The API

The full interface, with comments, is
[`extension.wit`](../crates/ferrite-extension-api/wit/extension.wit). Functions the extension
calls are in `host`; the extension exports the rest.

### Positions and versions

**Positions.** A position is a zero-based **line** and a **column in UTF-8 bytes** from the start
of that line. Rust strings index the same way. Ranges are `start` up to (not including) `end`.

**Versions.** Every document has a **version** that increases with each change. Reads return the
version they read. Edits and diagnostics name the version they refer to.

### Reading documents

- `documents()` and `document(id)` return each document's id, version, path, language, line
  count, length in bytes, and whether it is unsaved.
- `read-lines(id, start, end)` returns lines without their line breaks. It may return fewer
  lines than asked for: at most 10,000 lines or 4 MiB at once, so loop until you have them all.
- `read-range(id, range)` returns the text of a range, up to 4 MiB.

Read only what you need. Tidy reads whole documents once, then only the lines each change
touches.

### Notifications

- `document-opened(info)` is sent for every open document when the extension starts, and for
  each document opened later.
- `document-changed(change)` lists the changes since the previous notification, in order. Each
  change is a range in the text before it, plus the text that replaced it, like LSP incremental
  sync but in UTF-8 columns. If an extension falls behind, waiting changes are merged. If they
  grow past 1 MiB, or events had to be dropped, `changes` is `none`: read the document again.
- `document-closed(id)` is sent when a document closes.

Notifications are delivered one at a time, in order, on the extension's own thread. The editor
never waits for them.

### Editing

`apply-edits(id, version, edits)` applies all the edits or none. The host checks each of these
first, and returns a specific error when one fails:

- the extension has `documents_write` (`permission-denied`);
- the document is open (`unknown-document`);
- it is still at `version` (`stale-version`, carrying the current version);
- every position is inside the document and on a character boundary (`invalid-range`);
- edits don't overlap (`invalid-range`);
- sizes are within limits: at most 10,000 edits and 16 MiB of inserted text (`too-large`).

Ranges refer to the text at `version`, so they don't shift as earlier edits apply. Line breaks
in inserted text are converted to the document's line ending. The whole call becomes **one undo
step**.

When the user is typing, edits based on a slightly old version fail with `stale-version`. Read
again and retry, as Tidy's trim command does (three attempts).

### Diagnostics

`publish-diagnostics(id, version, list)` replaces the extension's diagnostics for a document.
Each diagnostic has a range, a severity, a message, and an optional code.

- **Version.** It may be the current version or one of the last eight sent to the extension.
  Ranges are converted in that text, then move with later edits.
- **Limit.** At most 5,000 diagnostics per call.
- **Display.** They appear as squiggles, in hover cards (labeled with the extension's name), and
  in the status bar's counts for files with a known language.
- **Removal.** They are removed when the extension stops.

### Settings, logging, and messages

**Settings.** `configuration(key)` returns the user's value for a declared setting, or its
default. Users set values in `settings.toml`:

```toml
[extensions.settings."example.tidy"]
max_line_length = 120
```

Values are checked against the declaration. Invalid values are listed in the Extensions view and
the default is used instead. Values are read at call time, so a changed setting applies on the
next read.

**Logging and messages:**

- `log(level, message)` writes to Ferrite's log, tagged with the extension's id. Messages are cut
  at 4,000 characters, and at most 50 per second are kept, after a burst of 200.
- `show-message(level, message)` shows a notification. Messages are cut at 1,000 characters, and
  at most 1 per second is shown, after a burst of 10.

## Workbench contributions

Contributions are declared in the manifest, and the workbench draws them itself. An extension
never supplies markup or CSS: it supplies text, colors that must be hex colors, and icons that
are shown as images.

- **Ids.** Contribution ids are local to the extension (`list`, `count`). In the workbench they
  become `<extension id>.<id>`.
- **Commands.** Commands named by contributions must be the extension's own.
- **Disabled extensions.** Contributions of a disabled or invalid extension aren't shown.

### Themes

```toml
[[themes]]
id = "midnight"                   # used as "example.themes.midnight"
label = "Midnight"
kind = "dark"                     # the built-in theme it starts from: dark or light
path = "themes/midnight.toml"     # inside the extension's folder
```

The theme file sets Ferrite's color tokens; any token it leaves out keeps the base theme's
color:

```toml
[colors]
editor-background = "#1a1b26"
syntax-keyword = "#bb9af7"
```

- **Tokens.** The tokens are listed in [`src/themes/tokens.json`](../src/themes/tokens.json)
  and defined in `src/styles/tokens.css`.
- **Validation.** Unknown tokens and values that aren't hex colors make the extension invalid.
- **No code needed.** An extension that only contributes themes needs no entrypoint.
- **Selecting.** Users pick a theme with **Preferences: Color Theme: …** or
  `workbench.theme = "example.themes.midnight"`.
- **User colors.** `[workbench.colors]` in the settings overrides individual tokens on top of
  any theme.

### File icon themes

An icon theme replaces the icons the explorer and Go to File show for files and folders
([ADR 0009](adr/0009-file-icon-themes.md)). [`extensions/icons`](../extensions/icons/) is a
complete example.

```toml
[[icon_themes]]
id = "colorful"                   # used as "example.icons.colorful"
label = "Colorful"
path = "icons/theme.toml"         # inside the extension's folder
```

The theme file names its icons, then says which icon goes with what. Icon paths are relative to
the theme file.

```toml
file = "file"                     # files nothing below matches
folder = "folder"                 # folders nothing below matches
folder_expanded = "folder-open"

[icons]                           # icon id = SVG or PNG file
file = "file.svg"
folder = "folder.svg"
folder-open = "folder-open.svg"
rust = "rust.svg"
typescript = "typescript.svg"
typescript-def = "typescript-def.svg"
folder-src = "folder-src.svg"
folder-src-open = "folder-src-open.svg"
folder-dark-ink = "folder-dark-ink.svg"
rust-dark-ink = "rust-dark-ink.svg"

[file_extensions]                 # without the leading dot
rs = "rust"
ts = "typescript"
"d.ts" = "typescript-def"         # the longest match wins

[file_names]                      # exact names; these beat extensions
"cargo.toml" = "rust"

[folder_names]
src = "folder-src"

[folder_names_expanded]           # open folders; falls back to folder_names
src = "folder-src-open"

[light]                           # looked up first with a light color theme
folder = "folder-dark-ink"
file_extensions = { rs = "rust-dark-ink" }
```

- **Matching.** Names and extensions are matched ignoring case. A leading dot starts a name, not
  an extension: `.eslintrc.json` matches the extension `json`.
- **Fallback.** Anything the theme doesn't match uses its `file`, `folder`, and
  `folder_expanded` icons. If it doesn't set those either, Ferrite's built-in icons are used, so
  a theme can change just a few file types.
- **Icons.** Icons are SVG or PNG files, shown at 16 × 16 pixels. They are shown as images, so
  they can't use theme colors. Provide light variants under `[light]` if an icon needs one.
  Scripts and external references in an SVG are ignored.
- **Validation.** Icons outside the theme's folder, files that aren't SVG or PNG, and
  associations that name an undeclared icon make the extension invalid.
- **No code needed.** An extension that only contributes themes and icon themes needs no
  entrypoint.
- **Selecting.** Users pick an icon theme with **Preferences: File Icon Theme: …** or
  `workbench.icon_theme = "example.icons.colorful"`. **File Icon Theme: Default** returns to
  the built-in icons.

### Keybindings

```toml
[[keybindings]]
command = "example.todos.next"
key = "ctrl+alt+t"                # modifiers: mod, ctrl, alt, shift, meta
mac = "mod+alt+t"                 # optional; "mod" is Cmd on macOS
when = "editorFocus"              # optional: only while the editor has focus
```

Keys are written like `ctrl+shift+p`, `alt+f12`, or `mod++`. Named keys are `enter`, `tab`,
`escape`, `space`, `backspace`, `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`, the
arrows (`arrowup`, …), and `f1` to `f24`.

**Precedence.** A built-in command's keys win over an extension's. The user's `[keybindings]`
settings win over both: they replace a command's keys, or remove them with `""`. **Preferences:
Keyboard Shortcuts** lists every command with its id and keys.

### Menus

```toml
[[menus]]
location = "editor/context"       # or "explorer/context"
command = "example.todos.insert"
group = "todos"                   # items are grouped by this, then sorted by title
```

- **`editor/context`** items appear in the editor's right-click menu and run with the usual
  command context (the active document and its selections).
- **`explorer/context`** items appear when right-clicking a file in the explorer. They receive
  the file's path as the command argument (`workbench::command-argument`).

### Status bar items

```toml
[[status_items]]
id = "count"
alignment = "right"               # or "left" (default)
priority = 10                     # higher is further left within its side
text = "TODOs"                    # shown until the extension sets other content
tooltip = "Show TODOs"
command = "example.todos.show"    # optional: run on click
```

The item shows its manifest text even before the extension starts (clicking it starts the
extension). While running, the extension changes it with `workbench::set-status-item(id, item)`,
or hides it with `none`. Text is plain text, at most 100 characters.

### Side bar views

```toml
[[views]]
id = "list"
title = "TODOs"
icon = "check"                    # list, check, bolt, book, bug, tag, star, flag, bell
```

A view adds an activity bar entry and a side bar panel. Its content is a tree of items:

- **`set-view-items(view, items)`** sets the tree, at most 5,000 items. Each item has an id, an
  optional parent (which must come earlier in the list), a label, a description, a tooltip,
  whether it starts expanded, and an optional command and argument.
- **Activating an item** (click or Enter) runs its command. The command reads the item's
  argument with `workbench::command-argument()`.
- **`set-view-message(view, message)`** shows a line of text above the items, such as "No TODOs
  in open documents."
- **`show-view(view)`** brings the view into the side bar.

Use `on-view:<id>` to start the extension only when its view is first shown.

### Decorations

```toml
[decorations.keyword]
background = "#e5c07b26"          # all colors optional; hex only
color = "#e5c07b"                 # text color
border = "#e5c07b"
underline = "wavy"                # solid, wavy, dotted, or dashed
underline_color = "#e5c07b"
bold = true
italic = false

[decorations.keyword.light]       # optional colors for light themes
background = "#9868011f"
color = "#986801"
```

`workbench::set-decorations(document, version, "keyword", ranges)` replaces the ranges decorated
with that type in a document (at most 10,000). It works like `publish-diagnostics`:

- **Version.** Ranges refer to the text at `version`, which may be the current version or a
  recently sent one.
- **Editing.** Decorations move with later edits.
- **Removal.** Decorations are removed when the extension stops.
- **Permission.** It needs the `documents_read` permission.

`workbench::reveal(document, range)` selects a range and scrolls it into view, making that
document active. TODOs uses it when a view item is chosen. It also needs `documents_read`.

### Limits

| Item                               | Limit                                   |
| ---------------------------------- | --------------------------------------- |
| Themes / keybindings / menus       | 20 / 100 / 50 per extension             |
| Icon themes                        | 5 per extension                         |
| Icons                              | 2,000 per theme, 64 KiB each, 8 MiB all |
| Icon associations                  | 10,000 per theme                        |
| Status items / views / decorations | 20 / 10 / 20 types per extension        |
| Status text / tooltip              | 100 / 500 characters (cut, not refused) |
| View items                         | 5,000 per view                          |
| Decorated ranges                   | 10,000 per type and document            |

Misuse fails with `invalid-argument` or `too-large` and changes nothing: an undeclared id, a
command that isn't the extension's own, or an item whose parent isn't listed before it.

## Languages

Every language Ferrite knows comes from an extension: which files it is for, syntax
highlighting from a [Tree-sitter](https://tree-sitter.github.io/) grammar, highlighting of other
languages embedded in it, and its language server. None of this needs code, so a language-only
extension has no entrypoint. [HTML & CSS](../extensions/html-css/) is the example used below;
the [Rust](../extensions/rust/), [TypeScript](../extensions/typescript/) (two languages, TS and
TSX), [JavaScript](../extensions/javascript/), [JSON](../extensions/json/),
[TOML](../extensions/toml/), and [Python](../extensions/python/) extensions are simpler ones.

```text
html-css/
  extension.toml
  grammars/
    tree-sitter-html.wasm         Tree-sitter grammars, compiled to WebAssembly
    tree-sitter-css.wasm
  queries/
    html/highlights.scm           what to color
    html/injections.scm           where CSS and JavaScript are embedded
    html/folds.scm                what folds
    css/highlights.scm
    css/folds.scm
```

```toml
[[languages]]
id = "html"                       # the language id (not prefixed with the extension's id)
name = "HTML"                     # shown in the status bar
file_extensions = ["html", "htm"] # without the dot; may contain dots, like "d.ts"
file_names = []                   # whole file names, like "Makefile"
grammar = "grammars/tree-sitter-html.wasm"
grammar_name = "html"             # optional: the id, with '-' as '_'
highlights = "queries/html/highlights.scm"
injections = "queries/html/injections.scm"   # optional
folds = "queries/html/folds.scm"  # optional; without it, the language folds by indentation
block_comment = ["<!--", "-->"]   # optional; for Toggle Line Comment
# line_comment = "//"             # optional; preferred over block_comment when both are set
```

All paths must be inside the extension's folder, and the files must exist when the extension
loads. The language id is what language servers, `on-language:` activation, settings, and other
languages' injections use, so it is global: a language whose id another extension already
registered is left out (the log says so). Ids are 1 to 32 lowercase
letters, digits, `-`, and `_`.

**Comments.** **Toggle Line Comment** (Ctrl+/) uses `line_comment`, or wraps the lines in
`block_comment` for languages without line comments. Each delimiter is 1 to 8 characters without
spaces. Without either, the command does nothing for the language. The same tokens find region
comments (`// #region Name` … `// #endregion`, or `/* #region Name */` with a block comment)
and `MARK:` comments, so a language gets foldable regions and minimap labels by declaring its
comment syntax.

**Which language a file gets.** A whole file name beats an extension, and a longer extension
beats a shorter one (`d.ts` beats `ts`). When two languages claim a file equally, the one
registered first wins (extensions are loaded in order of name). Names are matched
case-insensitively.

### Grammars

A grammar is a Tree-sitter parser compiled to WebAssembly with `tree-sitter build --wasm`, which
exports `tree_sitter_<grammar_name>`. Ferrite uses Tree-sitter 0.27, which loads grammars of
language ABI 13 to 15. Many grammars publish a prebuilt `.wasm`: all the examples' grammars are the
files from their npm packages (`tree-sitter-html@0.23.2`, `tree-sitter-css@0.25.0`,
`tree-sitter-rust@0.24.0`, and so on; each extension's `grammars` folder has the license). To build one yourself:

```sh
npx tree-sitter-cli@0.27.1 build --wasm path/to/tree-sitter-html
```

A grammar runs in its own WebAssembly sandbox, separate from extensions'. It can use only
Tree-sitter's small C library and up to 128 MiB of memory per thread that parses it. It is
loaded and compiled the first time a file of its language is opened, not when Ferrite starts.

**Grammars are trusted not to loop.** Ferrite stops a parse that takes too long between steps,
but it can't interrupt a grammar's external scanner in the middle of a call. A scanner that
never returns blocks the thread parsing, which can freeze the editor. Use grammars from sources
you trust, as you would any program.

### Highlight queries

`highlights.scm` is a standard Tree-sitter
[highlight query](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html#highlights):
patterns that capture nodes with names like `@keyword` or `@function.method`. Ferrite colors a
capture by the first part of its name, using the theme's `syntax-*` colors:

| Capture names                                              | Color     |
| ---------------------------------------------------------- | --------- |
| `comment`                                                  | comment   |
| `keyword`, `include`, `conditional`, `repeat`, `exception` | keyword   |
| `string`, `character`                                      | string    |
| `string.escape`, `escape`                                  | escape    |
| `number`, `float`, `constant.numeric`                      | number    |
| `constant`, `boolean`                                      | constant  |
| `function`, `method`, `constructor`                        | function  |
| `macro`, `function.macro`                                  | macro     |
| `type`                                                     | type      |
| `variable.builtin`                                         | variable  |
| `property`, `field`                                        | property  |
| `operator` / `punctuation` / `attribute` / `tag` / `label` | same name |
| `module`, `namespace`                                      | module    |

Other captures, including plain `@variable`, are left uncolored. Where captures overlap, the
narrower one wins, and for the same node the earlier pattern wins, as highlight queries
expect. Predicates such as `#eq?` and `#match?` work.

### Folding

`folds.scm` captures foldable nodes as `@fold`:

```scheme
[
  (block)
  (declaration_list)
  (arguments)
  (block_comment)
] @fold
```

A region is the captured node's lines. Its first line stays visible when it is folded, and the
rest are hidden. The exception is a last line that closes the first: it starts with `}`, `)`,
`]`, or `</`, the first line ends with an opening bracket or `>`, and it is indented no deeper.
That line stays visible too, so folded code reads `fn main() {⋯` followed by `}`. Capturing
either a body or the whole item gives this shape. Regions that would hide nothing are skipped,
and injected languages fold by their own queries. A language without `folds.scm` folds by
indentation.

### Injections

`injections.scm` marks text in another language, which Ferrite parses with that language's
grammar and highlights over the outer language:

```scheme
((style_element (raw_text) @injection.content)
 (#set! injection.language "css"))
```

- **`@injection.content`** is the embedded text. The whole node is used, children included.
- **The language** is a language id, set with `#set! injection.language`, or taken from the
  text of a node captured as `@injection.language` (for example a Markdown code fence's info
  string). It may be the extension's own language or another extension's; an id no language
  has is ignored (HTML's scripts are highlighted only if the JavaScript extension is loaded).
- **`#set! injection.combined`** parses all of a pattern's matches together, as one document.
- **Limits.** Injections are followed 4 levels deep (CSS in HTML in Markdown, say), and a
  document has at most 1,024 injected regions.

Each injected region is parsed incrementally as you type, within the same 3 ms budget as the
document itself; anything longer finishes in the background.

### Language servers

```toml
[permissions]
language_servers = true

[[language_servers]]
name = "vscode-html-language-server"     # unique; also its name in the settings
command = "vscode-html-language-server"  # a program on the PATH, or a path in the folder
args = ["--stdio"]
languages = ["html"]                     # the language ids it serves
root_markers = ["package.json"]          # optional: files marking a project root
```

- **Permission.** Declaring language servers requires `language_servers = true`, and the other
  way around. Ferrite asks the user before running them, showing each command line. Until the
  user allows it, or if they deny it, the servers don't run; the languages are highlighted
  either way.
- **Ferrite runs them**: started on demand when a file of
  one of their languages is opened, with hover, go to definition, completion, and diagnostics.
  The extension's code, if any, has no access to them.
- **Commands.** A command without `/` or `\` is looked up on the `PATH`. A relative path runs a
  program the extension ships, inside its folder. Absolute paths aren't allowed. A server that
  isn't installed shows as failed in the status bar, with a hover explaining why.
- **Names.** Extensions that declare a server with the same name and command share one
  server for all their languages: the TypeScript and JavaScript examples both declare
  `typescript-language-server`. With a different command, the first declaration wins. Users can
  change an extension's server or turn it off by name, for example
  `[lsp.servers.rust-analyzer]` with `enabled = false` or another `command`.

### Trying it

1. Run **Extensions: Load Development Extension…** and choose `extensions/html-css`. Nothing
   needs building.
2. Ferrite asks whether to allow the two language servers. Either answer is fine; to get them,
   run `npm install -g vscode-langservers-extracted`.
3. Open an `.html` file: tags, attributes, and comments are colored, and so is the CSS in
   `<style>` and the JavaScript in `<script>`.

After editing a query, click **Restart** in the Extensions view: languages whose files changed
are loaded again, and open documents are highlighted with them. An invalid query is logged
(`syntax highlighting is unavailable`, with the query's error) and leaves the file uncolored.

### Language limits

| Item                         | Limit                    |
| ---------------------------- | ------------------------ |
| Languages / language servers | 20 / 10 per extension    |
| File extensions / names      | 50 / 50 per language     |
| Grammar / query file         | 32 MiB / 1 MiB           |
| Grammar memory               | 128 MiB per thread       |
| Injections                   | 4 deep, 1,024 a document |

## Limits and failures

Each extension runs on its own thread with these limits:

| Limit                          | Value                      |
| ------------------------------ | -------------------------- |
| Memory (linear memory)         | 128 MiB                    |
| Memory of all extensions       | 512 MiB together           |
| Instantiation and `activate`   | 5 s                        |
| One command                    | 10 s                       |
| One notification, `deactivate` | 1 s                        |
| WebAssembly stack              | 1 MiB                      |
| Waiting events                 | 1,024 (changes are merged) |

**Failures.** Ferrite stops an extension when it traps, panics, runs out of memory or stack, or
misses a deadline. Infinite loops are interrupted at the deadline. The extension is marked
failed, and its diagnostics, decorations, status bar items, and view contents are removed. The
user sees why, with a **Restart** button. The editor and the document are unaffected.

**Shared memory.** Running extensions share a 512 MiB memory pool, besides each one's own
limit. An extension whose memory can't grow because the pool is used up fails with a message
saying so, and a stopped extension returns its memory to the pool.

**Resource use.** Ferrite counts, per extension, how often it ran and for how long, its slowest
call, its memory (current and peak), the document changes it skipped because its queue was
full, and how long it took to compile. Users see this in the Extensions view and in **Developer:
Toggle Performance Overlay**. A call that takes more than a quarter of its deadline is logged as
`slow extension call`. Keep handlers short: do the minimum per change, and use the changed
ranges instead of re-reading the whole document.

**Commands.** A command that returns `Err(message)` shows the message as an error; the
extension keeps running.

**Restarts.** A failed extension doesn't restart by itself. Restarting creates a fresh instance:
nothing persists between instances.

Every failure listed here is covered by the host's integration tests, which use the
[misbehaving test extension](../extensions/test-fixtures/misbehaving/).

## Installing

Ferrite loads installed extensions from the `extensions` folder next to `settings.toml`, for
example `%APPDATA%\dev.ferrite.editor\extensions` on Windows. Each extension is a subfolder with
its `extension.toml` and `.wasm` file:

```text
extensions/
  tidy/
    extension.toml        entrypoint = "tidy.wasm"
    tidy.wasm
```

If an installed extension and a development extension share an id, the development extension
wins. Set `extensions.enabled = false` to load none.

## Testing

**Unit tests.** Test your logic as ordinary Rust on your host target. The host functions are
unavailable there, so keep them at the edges:

```sh
cd extensions
cargo test -p tidy --target x86_64-pc-windows-msvc    # or your host's target triple
```

**Integration tests.** The extension host's tests (`crates/ferrite-extension-host/tests/host.rs`)
build the extensions workspace and run the extensions against a real editor core. They are the
model for testing an extension end to end. Run them with
`cargo test -p ferrite-extension-host`.

## Other languages

Any language that can produce a WebAssembly component for the `extension` world works. The
component must import nothing but the Ferrite interfaces (`ferrite:extension/host` and
`ferrite:extension/workbench`). The host provides no WASI, so
a component that imports `wasi:*` interfaces (most toolchains' defaults) fails to start, with a
message saying so. Rust on `wasm32-unknown-unknown` is the supported path for now.

## Versioning

The WIT package is versioned: `ferrite:extension@1.1.0`, `api_version = "1"`.

- **Additions.** New host functions arrive as minor versions of the package. Version 1.1 added the
  `workbench` interface. Wasmtime links components built against an older minor version: the
  host's tests load a Tidy build made against 1.0.
- **Breaking changes.** Changes that break existing extensions get a new major version and a new
  `api_version`.
- **Rejection.** Manifests for unsupported versions are rejected before loading.
