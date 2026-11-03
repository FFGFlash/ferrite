# ADR 0024: A document in several groups

- **Status:** Accepted
- **Date:** 2026-11-02

## Context

A document showed in one group at a time ([ADR 0012](0012-docking.md)). VS Code lets the same file
show in several editor groups, once in each, so one part of a file can be read while another is
edited. Each of those editors has its own cursor, scroll position, and folds, and they share the
text, its undo history, and whether it's saved.

The backend kept all of that per document: the selections, cursor history, and folds in
`ferrite_core::Document`, and what the client had been sent of the highlights (the highlight
mirror). Two editors of one document would have fought over all of it.

## Decisions

### Views, in the core

A document has views (`ViewId`), one per editor showing it. Each has its own selections, cursor
history, and folded regions; everything else is shared. The document holds the view in use in the
fields it always had, so the editing operations didn't change, and parks the others:

- **`use_view`** swaps a view in. A view not seen before starts as a copy of the one in use.
- **Edits follow into parked views.** After every change, the parked views' selections are mapped
  through it and their folds synced, as the view in use's are. A cursor where another view
  inserted text stays before it.
- **`copy_view`** makes one view a copy of another, for an editor split off another.
- **`close_view`** forgets one. A document always keeps a view.

Undo is shared, so undoing in one editor an edit made in another restores the selections from that
edit, as in VS Code.

Alternatives considered:

- **Selections and folds keyed by view throughout the core.** Every operation would take a view.
  Swapping keeps the ~180 places that use the selections and folds as they were.
- **Shared cursors.** Simpler, but clicking in one editor would move the cursor in the other.

### The commands carry the view

`editor_view`, `editor_apply`, `editor_clipboard`, and `editor_minimap` take a `view` (0 if not
given) and use it before anything else. `editor_copy_view` and `editor_close_view` copy and forget
views. The highlight mirror is kept per view, and so is "this view still shows colors of a language
the document lost". With one mirror, a refreshed highlight sent to one editor would have been
counted as sent to another holding the same lines.

### Tabs

A document's tabs are `document:<id>` (view 0) and `document:<id>:<view>`. Every tab is still in
exactly one group, so the dock model is unchanged, and the dock numbers views.

- **Split Tab Right/Down** copies a document into the new group, as VS Code's Split Editor does:
  the same selections, folds, and scroll position. Other items still move.
- **Dragging with the copy key** (Ctrl, or Option on macOS) copies a document's tab the same way.
- **Once per group.** A document tab moved into a group that shows the document joins the tab there.
  Reset Layout keeps each document's tab in use.
- **Closing** one of a document's tabs closes only that view, without asking about unsaved changes.
  Closing its last tab closes the document.
- **Reopen Editor With** changes only the tab it's used on. A viewer takes one tab's place while the
  document stays open in its others, and a viewer reopened as text, of a document open elsewhere,
  shows it there in a view of its own.
- **Opening a file** (the explorer, Go to File, a search result, a definition) that shows in
  another group opens it in the group documents open in too, as VS Code does by default. Switching
  to a document (its tab, the activity bar, an extension's reveal) shows it where it is: in the
  group documents open in if it's there, else in the tab last used.
- **Dropping a file** on a group, or reopening a closed tab, opens it there even if it shows
  elsewhere (it moved there before).

### One editor in use per document

The frontend registers editors by document and view. The dock tells it which tab of each document
was last used (`setShownView`). That tab's editor is the one commands, selections from search
results, and focus go to (`editorFor`). Events about a document refresh all its editors
(`editorsFor`), and an editor that changes the text refreshes the others. The status bar follows
the active editor only, so an editor refreshing because another was edited doesn't take it over.
The references peek remembers its view too.

### Saved by path, copies too

A document's first tab in reading order is saved as `file:<path>`, the others as
`copy:<n>:<path>`. When the document opens again, the first becomes view 0 and the others get
views of their own, copies of view 0 (views aren't saved).

## Consequences

- The "one document, one place" consequence of ADR 0012 no longer holds.
- A tab whose view the backend doesn't know (after a restart, say) gets one copied from the view in
  use when it first asks for lines.
- Remembered folds (`fold_memory`) are the folds of the view last folded in.
- Extensions and language servers see one document, as before. What they read of its selections
  (backups' cursor position, for one) is the view in use's.
