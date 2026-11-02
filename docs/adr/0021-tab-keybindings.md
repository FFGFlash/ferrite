# ADR 0021: Closing, reopening, and moving between tabs from the keyboard

- **Status:** Accepted
- **Date:** 2026-11-02

## Context

`Ctrl+W` was bound to Close Editor, which closes the active document wherever focus is. Since
docking ([ADR 0012](0012-docking.md)) made views, pages, and terminals tabs too, that was the wrong
target: with focus in the explorer, `Ctrl+W` closed a document out of sight. There was no way to
reopen a closed tab, and several VS Code keys people expect (bulk closes, Save All, jumping to a
group or tab by number, zoom) were missing or had commands with no keys.

## Decisions

### `Ctrl+W` closes the current group's tab

`Ctrl+W` now runs **View: Close Tab**, which closes the active tab of the current group (the
group focus was last in, `currentGroup`), whatever its kind. **Close Editor** stays, unbound,
for closing the active document from anywhere.

In a terminal, `Ctrl+W` still goes to the shell, where it deletes a word ([ADR
0011](0011-integrated-terminal.md)). Terminals close from their tab (×, `Delete`, the tab menu) or
**Kill Terminal**.

### Reopen Closed Tab (`Ctrl+Shift+T`)

The dock keeps the last 20 closed tabs, in memory, with the group and index each had:

- **Documents** are kept by path (`file:<path>`), only once they've really closed (closing one
  with unsaved changes can be cancelled). Reopening puts the path in its old place first, so the
  document takes it as it opens, the same way saved layouts restore (`syncDocuments`). If the
  file fails to open, the place is removed.
- **Views and pages** reopen where they were.
- **Not kept:** terminals (closing ends the shell, so a "reopened" one would be a different
  shell), previews (their extension owns them), and untitled documents (their text is gone).

Entries for tabs that are open again are skipped. If the group is gone, the tab opens where it
would by default.

### Bulk closes

**Close Other Tabs**, **Close Saved Tabs in Group** (`Ctrl+K U`), and **Close All Tabs in Group**
(`Ctrl+K W`) act on the current group. **Close All Editors** (`Ctrl+K Ctrl+W`) closes every
document, preview, and page, leaving views and terminals: closing all terminals would end every
shell.

Tabs close one at a time, from the last, so each is kept at its real index and Reopen Closed Tab
brings them back in their order. Cancelling one close (an unsaved-changes prompt) stops the rest.
**Save All** (`Ctrl+K S`) saves each document with unsaved changes in turn.

### By number

- **`Ctrl+1` … `Ctrl+9`** focus the Nth visible group outside the side bar, in reading order. In
  the default layout `Ctrl+1` is the main group, as in VS Code, where the number counts editor
  groups only.
- **`Alt+1` … `Alt+9`** (`⌃1` … on macOS) show the Nth tab of the current group, `Alt+0` the last.
  They aren't bound in terminals, where readline uses `Alt+digit`.

### Zoom

**Zoom In / Out / Reset Zoom** (`Ctrl+=`, `Ctrl+-`, `Ctrl+0`) set the webview's zoom, in the
steps browsers use (50% to 300%). It's saved with the layout (`zoom`) and applied when the layout
loads. That needs Tauri's `core:webview:allow-set-webview-zoom`, the first core permission the
frontend is granted. CSS `zoom` was the alternative, but it changes how elements measure, which the
editor, minimap, and terminal depend on.

### Keys for commands that had none

Split Tab Right (`Ctrl+\`), Move Tab Left/Right (`Ctrl+Alt+←/→`, `⌃⌘←/→` on macOS), Keyboard
Shortcuts (`Ctrl+K Ctrl+S`), Open Folder (`Ctrl+K Ctrl+O`), and Close Folder (`Ctrl+K F`). Move Tab
Up/Down stay unbound: `Ctrl+Alt+↑/↓` adds cursors in the editor.

A test checks that no two built-in commands share a key on either platform, and that no single
key is also the start of a chord.

## Consequences

- `Ctrl+0` is reset zoom, so VS Code's Focus Side Bar key isn't used. `F6` and the activity bar
  reach the side bar.
- Closed-tab history doesn't survive a restart or a window reload.
- A group emptied by closing its tabs is removed, so its tabs reopen in the default places rather
  than in a recreated group.
