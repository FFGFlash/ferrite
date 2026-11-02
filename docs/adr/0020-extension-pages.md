# ADR 0020: The Extensions view and extension pages

- **Status:** Accepted
- **Date:** 2026-11-02

## Context

The Extensions view showed everything about every extension at once: state, description,
contributions, resource use, permission checkboxes, command buttons, the folder, and action
buttons. Each entry was a different height, and a few extensions filled the side bar. An
extension also had no way to say who made it, show a picture, or explain itself beyond a
500-character description.

## Decisions

### A list, and a page per extension

The view is a list of rows of one height: icon, name, a one-line description, author, how long
it took to start, and its state when that needs attention (stopped, invalid, needs permission).
What's cut is in the row's tooltip. A gear (and right-click) has the actions: Show Details,
Enable or Disable, Restart, Settings, Show Log, Copy Extension ID.

Choosing a row opens the extension's page, a dock page like the settings editor
(`page:extension:<id>`), where documents open. It has the README, and beside it (below, when
narrow) the permissions, commands, contributions, resource use, and folder. Pages are found by
id while the extension is loaded, with one component per extension, so a status update doesn't
remount the README.

### Manifest: `author` and `icon`; the README by convention

- **`author`** is free text, up to 100 characters. There's no publisher registry to check it
  against.
- **`icon`** is a PNG or SVG file in the extension's folder, up to 256 KiB, checked when the
  manifest loads. It's loaded on request (`extension_icon`) and cached by the frontend, rather
  than sent with every status update.
- **The README** is `README.md` in the extension's folder, found whatever its case, as on
  package registries, so there's nothing to declare.

### The README goes through the preview pipeline

The Markdown extension already turns Markdown into preview nodes. The extension host compiles
that same file (`extensions/markdown/src/render.rs`, through `#[path]`) against its own copy of
the preview types, which the file reaches through its parent module's `types`. The nodes then go
through `preview::convert`, like any extension's preview: checked, links and pictures resolved
from the extension's folder, and nothing turned into markup. The frontend draws them with the
preview's renderer (`PreviewDocument`, split out of `PreviewPane`), with code highlighted the same
way.

- **Not through the Markdown extension.** It may be disabled or not installed, and an
  extension's page shouldn't depend on another extension.
- **Not a second renderer.** Two Markdown renderers would drift apart. Sharing the file also
  means its tests run in both workspaces.
- **Pictures** load only from the extension's folder (checked after resolving links), with the
  preview's size and type limits. Web pictures aren't loaded, as in previews.

### Start time

`ResourceUsage::activation_ms` is the time from starting an extension to its `activate`
returning, compiling and instantiating included, most recently. It's what the user waits for,
and what an extension's author can reduce. It's `None` until the extension has started.

## Consequences

- Permissions are changed on the page, not in the list.
- The Markdown extension's renderer can only use the preview types and pulldown-cmark, since the
  host compiles it too. The file says so.
- `pulldown-cmark` is now a dependency of the extension host, in the version the Markdown
  extension uses.
