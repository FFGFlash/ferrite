# ADR 0007: Workbench contributions

- **Status:** Accepted
- **Date:** 2026-10-24

## Context

Milestone 7 asks for an initial contribution model so extensions can customize the workbench's
appearance and functionality without changes to the editor's source. That covers:

- themes;
- keybindings;
- menus;
- status bar items;
- side bar views;
- editor decorations.

The brief (§5.4) asks for host-managed, declarative contributions rather than extension-supplied
UI code, with no DOM access or arbitrary JavaScript. Extensions run as WebAssembly in the backend
(ADR 0006), so they can't render React components anyway.

## Decisions

### Declare in the manifest, fill in through the API

Every contribution is declared in `extension.toml`, so the workbench knows about it before the
extension runs.

- **What the workbench shows up front.** Activity bar entries, palette commands, keybindings,
  menu items, and status bar text all appear without the extension running.
- **What extensions supply.** They provide only content: status text, view items, and decorated
  ranges. They do this through a new `workbench` interface, and every value is plain text, a
  range, or a declared id.
- **Hidden commands.** Commands that exist only for contributions (a view item's action) can be
  `hidden` from the palette.

Alternatives considered:

- **A structured UI protocol** (extensions emit element trees, receive events), as the brief
  suggests investigating for rich views. It is more capable, but much larger to specify and
  secure. A tree of labelled items with commands covers the common cases (outlines, problem
  lists, TODOs), and a richer protocol can be added as its own contribution type later.
- **Sandboxed webviews per extension.** Ruled out by the brief as the default (memory, security
  surface).

### API 1.1: additive, so 1.0 extensions keep working

The new functions live in a new interface, `ferrite:extension/workbench@1.1.0`, imported by the
same world. The world's exports are unchanged.

- **Old components.** A component built against 1.0 doesn't import `workbench`, and its `host`
  import (1.0.0) is satisfied by the host's 1.1.0 instance through Wasmtime's semver matching.
  The tests load a Tidy build made against 1.0.
- **View item arguments.** Activating a view item needs to tell the extension _which_ item.
  Rather than change `command-context` (a record; changing it would break 1.0 components) or
  add an export (which 1.0 components lack), the item's argument is available during the command
  through `workbench::command-argument()`. Explorer menu items pass the file's path the same way.

### Themes are data, validated twice

A theme names a base (dark or light) and overrides semantic color tokens.

- **Token list.** The tokens are listed in `src/themes/tokens.json`. The backend includes that
  file to validate themes, and a frontend test checks it against `styles/tokens.css`, so the two
  can't drift.
- **Values.** Values must be hex colors. The frontend applies them as CSS custom properties on
  the root element, checking token and format again, so nothing but a color can reach the
  stylesheet.
- **No code.** Theme-only extensions need no entrypoint and never run.
- **User colors.** `workbench.colors` in the settings overrides tokens on top of any theme.

### Keybinding precedence: user, then built-in, then extensions

Extensions declare keybindings for their own commands, in the same syntax as built-in ones
(normalized by `ferrite_extension_host::keys`).

- **Order.** When a key matches several commands, the user's `[keybindings]` settings win, then
  built-in defaults, then extension defaults. Installing an extension can't take Ctrl+S away,
  but the user can bind anything to anything, or unbind with `""`.
- **Discoverability.** **Preferences: Keyboard Shortcuts** lists every command with its id and
  keys.

### Menus: two locations, built-in items first

The editor's context menu (new) shows built-in items (Go to Definition, Cut, Copy, Paste,
Command Palette) with extension items in their own section, grouped and sorted. The explorer's
file context menu shows Open plus extension items, which receive the path. More locations (view
titles, tabs) can be added without changing the model.

### Decorations: types declared, ranges anchored like diagnostics

Decoration types (background, text color, border, underline style, bold, italic, with optional
light-theme colors) are declared in the manifest. Extensions set ranges per type and document.

- **Anchoring.** Ranges are stored against the text version they name and mapped through edits,
  exactly like extension diagnostics (shared code).
- **Views.** Each view carries the decorations on its lines, keyed by type.
- **Drawing.** The frontend draws backgrounds, borders, and underlines as overlay rectangles (like
  find matches). Text color and font styles become styled runs within the line, split by the same
  `segments` function as syntax highlighting.

### Output is the extension's to keep current, and goes away with it

Status content, view content, and decorations live in the host's state. They are removed when
the extension stops or fails, so a crashed extension leaves no stale UI behind. Updates are
compared with the current content and dropped when unchanged, so an extension that republishes
the same state on every keystroke causes no events.

## Consequences

- **Customization without code.** Extensions can now restyle the workbench (themes), add
  navigation (views, menus, keybindings), show status, and annotate text (decorations), all
  without editor changes. The TODOs example uses all of them.
- **Limits of the model.** Views are trees of text items. Rich custom views, inline text
  (inlays), gutter icons, hover content from decorations, and menus in other locations are left
  for later contribution types.
- **Discoverability.** Keybindings and themes are now configurable by users too, which M3 left
  for this milestone.
- **Theme timing.** A contributed theme applies once its extension has loaded and the frontend
  has fetched the extension list, shortly after the window appears. Until then the base theme
  is shown.
