# ADR 0010: Minimap

- **Status:** Accepted
- **Date:** 2026-10-25

## Context

Users want a VS Code-style minimap: the document drawn small beside the editor, with a slider
over the visible lines, clicked or dragged to scroll.

The frontend holds only the editor's viewport. The backend owns the text, and keeping a
keystroke's work proportional to what changed is a tested budget (ADR 0004). A minimap shows
hundreds of lines at once, so it can't reuse the editor's view, and it must not slow typing.

## Decisions

### A separate command that sends shapes, not text

`editor_minimap(id, startLine, endLine)` returns each line as runs of non-blank columns with a
token kind (`[start, end, kind, ...]`):

- **Columns.** Runs are in visual columns, with tabs at the document's tab stops, and only the
  first 120 columns are kept.
- **Size.** A line costs a few numbers instead of its text, and nothing in it is shown as text.
- **No mirror.** Highlights come from the document's Tree-sitter tree through the same query as
  the editor, but they don't go through the editor view's highlight mirror. The editor's
  incremental updates are unaffected, and so is the per-keystroke budget test, which counts only
  the editor view.

Alternatives considered:

- **Widening the editor view.** Sending the minimap's lines with each view would make every
  keystroke and scroll carry hundreds of lines, which breaks the budget.
- **Sending text and highlighting in the frontend.** That would duplicate the grammars in
  JavaScript and move far more data.

### Built off the editor lock

A minimap highlights roughly ten times as many lines as the editor shows. Built under the editor
lock, it delayed edits and views in a debug build (p95 60 ms per request on a 200,000-line Rust
file).

The command takes a `Snapshot` under the lock instead, made of three cheap copies:

- the rope, which shares its nodes;
- the edited Tree-sitter tree, which is reference counted;
- the tab size.

It then highlights and builds the runs on a worker thread with the lock released. The time
spent under the lock is recorded separately as `editor.minimap.lock`.

### Fetched by window, throttled while typing

The frontend asks for the lines the minimap shows plus 100 on each side.

- **Scrolling** past those lines fetches right away.
- **Edits and refreshes** (new highlights, decorations) fetch at most every 100 ms, drawing
  the previous lines until then.
- **One request at a time.** A request that fails isn't repeated until something changes.

### Drawn on a canvas, with VS Code's geometry

Each line is 2 px tall and each column 1 px wide, so the minimap is 120 px wide. It is drawn on
a canvas at the device pixel ratio, in the theme's syntax colors.

- **Scrolling.** When the document, including the space for scrolling past the last line, is
  taller than the minimap, the minimap scrolls in proportion to the editor.
- **Slider.** The slider is shown on hover, as in VS Code. Clicking centers the clicked line,
  and dragging maps the slider's travel onto the editor's scroll range.
- **Placement.** The minimap lies over the editor's right edge, beside the scrollbar. The text
  gets matching right padding, and revealing the cursor treats that strip as hidden. The wheel
  over the minimap scrolls the editor.
- **Toggle.** **View: Toggle Minimap** shows or hides it. The choice is kept with the layout.

## Consequences

- **Pointer only.** The minimap is `aria-hidden` and has no keyboard interaction, because every
  way to scroll already works without it. Its slider gets an outline under forced colors.
- **Not yet shown.** Selections, find matches, diagnostics, and decorations don't appear in the
  minimap. They can be added as overlays from data the backend already has.
- **Approximate columns.** Wide characters count as one column, unlike the editor's grapheme
  columns. That is close enough at 1 px.
- **Unhighlighted large files.** Documents over 16 MiB have no syntax tree, so their minimap is a
  single color.
