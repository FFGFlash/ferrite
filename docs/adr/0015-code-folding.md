# ADR 0015: Code folding

- **Status:** Accepted
- **Date:** 2026-11-01

## Context

Editors let you collapse a block of code to its first line. VS Code folds by the language's
syntax where it can and by indentation otherwise, and it remembers what was folded. Flareite's
languages come from extensions as Tree-sitter grammars with queries (ADR 0013), so syntax-aware
folding should come from them too.

Folding changes what the editor shows: lines inside a folded region have no place on screen.
Until now the view protocol assumed one row per line. The client asked for a contiguous range
of lines and held exactly that range.

## Decisions

### Regions come from a fold query, or from indentation

A language can declare a fold query (`folds = "queries/folds.scm"`) that captures foldable nodes
as `@fold`. A region is the captured node's lines. The first line stays visible. The rest are
hidden, except a last line that closes the first, such as `}` under `fn main() {` or `</div>`
under `<div>`. A line counts as closing when it starts with a closing bracket (or `</`), the first
line ends with an opening one (or a tag's `>`), and it is indented no deeper. Folded code then
reads `fn main() {⋯` followed by `}`, as in VS Code. Trailing blank lines are left out of a
region, and regions that would hide nothing are dropped. Injected languages contribute their own
regions (CSS rules inside HTML fold).

Languages without a fold query, and files without a language, fold by indentation. A region is a
line followed by lines indented deeper, up to the next line that isn't. That gives the same
`}`-stays-visible shape for brace languages.

All the bundled language extensions ship fold queries.

Alternatives considered:

- **Capture names for the visible end (`@fold.end`).** It would be more precise, but every query
  would need to mark it, and existing `folds.scm` files from other editors use plain `@fold`. The
  closing-line rule handles the common shapes without that.
- **Indentation only.** It needs no queries, but it can't fold multi-line argument lists,
  comments, or code that isn't indented consistently.

### The core owns folded regions; callers supply foldable ones

`flareite-core` doesn't depend on `flareite-syntax`, so the fold operations take the document's
foldable regions as line ranges. The app looks them up for each operation, from the query or
from indentation (`editor/folding.rs`): only the regions around the cursors, unless the
operation needs them all (Fold All, Fold Level).

Folded regions live in the `Document`, like selections, since a document has one editor. They are
stored as byte offsets: the end of the first line and the end of the last hidden line. They
catch up with edits lazily through the buffer's change log, as syntax trees do, so no edit path
needs a hook.

- An edit that removes either line break unfolds the region, as does a new line typed at the end
  of its first line.
- A selection that ends up in hidden lines unfolds their region, after a search or an undo, for
  example. Cursor movement steps over hidden lines instead. Up and Down count display rows.
- Line operations treat a folded region as part of its first line, as VS Code does. Moving,
  copying, deleting, selecting, cutting, and copying the line take the whole region, and moving
  lines past a folded neighbour moves over all of it. Moved regions stay folded.

### Viewports count display rows

A display row is a visible line. The viewport is a range of rows. The view says which visible
line ranges it covers (`visible`), and the client holds exactly those lines. Each request lists
the ranges the client holds (`known.ranges`) instead of one range. The backend therefore never
has to assume which lines a client has across a fold change. Unfolding simply makes the backend
send the newly visible lines.

Highlights, the highlight mirror, and the minimap all work on visible line ranges. The view also
carries `rowCount`, the cursor's row (for scrolling it into view), the visible lines that start a
foldable region (gutter markers), the folded ones, and a fold generation that changes whenever the
folded regions do (so the minimap refetches).

Alternatives considered:

- **Folds in the frontend.** The client would hide lines itself, but it would still need every
  line of a viewport that spans a large folded region. Cursor movement and line operations,
  which live in the core, would also need to know about folds.
- **Placeholders for hidden lines in the client's cache.** It would keep the contiguous range,
  but a folded region of a million lines would cost a million placeholders, and highlight work
  for each.

### Folded regions are remembered per file

The UI state keeps each file's folded regions with its line count, for up to 100 files. They are
recorded after fold operations, on save, and on close. A reopened file gets them back if its line
count is unchanged, but only the regions that are still foldable regions. The restore waits for
the first view after the regions are known, which for a query language means after its first
parse.

### Region comments

Region comments mark regions of the user's own: `// #region Name` … `// #endregion`, written with
the language's comment syntax (the `line_comment` and `block_comment` its manifest declares), as
in VS Code. `// MARK: Name` comments only label a place. `// MARK: - Name` adds a separator.

- A marked region folds through its `#endregion` line. Marked regions join the regions from the
  query or indentation, and Fold All Regions (Ctrl+K Ctrl+8) and Unfold All Regions (Ctrl+K Ctrl+9)
  act on them alone.
- The markers (`RegionMarkers` in `flareite-core`) are kept per document and updated from the line
  splices, so a keystroke rereads only the lines it changed. Documents over 16 MiB, like for
  highlighting, have none.
- The minimap labels named regions and `MARK:` comments. The labels are 9 px, as in VS Code, and
  stack rather than overlap. The backend sends all headers with their kind, and the frontend
  leaves out the kinds the `editor.minimap_region_headers` and `editor.minimap_mark_headers`
  settings turn off, so changing a setting needs no refetch.

Deriving the markers from the comment tokens means no new manifest field. A language with
different region syntax (C#'s `#region` directive, say) would need one.

### Commands and chords

Fold and Unfold are Ctrl+Shift+[ and Ctrl+Shift+] (⌥⌘[ and ⌥⌘] on macOS). The rest use VS Code's
chords: Ctrl+K Ctrl+L toggles, Ctrl+K Ctrl+[ and Ctrl+K Ctrl+] fold and unfold recursively,
Ctrl+K Ctrl+0 and Ctrl+K Ctrl+J fold and unfold all, and Ctrl+K Ctrl+1 to 7 fold a level.

The command registry gains chords: a keybinding of two key combinations separated by a space. The
first key press waits, and the status bar says so. The next key press runs the command or
cancels the chord. Either way it is used up. Settings and extension manifests accept chords too.
Bindings with Alt and punctuation now match by the physical key, as Alt and a letter already did,
because Option changes the character on macOS.

## Consequences

- Each language extension can add a `folds.scm` file. `docs/extensions.md` describes the
  closing-line rule.
- Clients must send the line ranges they hold, and must place lines by row, not by line number.
- Unfolding a region costs a round trip for its lines. Folding costs nothing to send.
- Fold queries that capture a whole item (`function_item`) instead of its body still fold well,
  thanks to the closing-line rule. A body that starts on a later line (a K&R-style brace on its own
  line) folds from that line.
