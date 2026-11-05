# ADR 0022: Dropping files from other programs, and pointer drags

- **Status:** Accepted
- **Date:** 2026-11-02

## Context

Files dragged onto the window from another program (File Explorer, Finder) should open, wherever
they are, and dropping them on the explorer should copy them into the folder.

The webview can't tell where a dropped file is: an HTML5 `drop` gives `File` objects without
paths. Tauri's drop handler gives paths, but [ADR 0012](0012-docking.md) turned it off
(`dragDropEnabled: false`), because on Windows it replaces WebView2's drop handling and HTML5
drags inside the page stop working. Tabs, activity bar buttons, and explorer rows used HTML5
drags.

## Decisions

### Tauri's drop handler on, and drags on pointer events

Tauri's drop handler is on again, on every platform. Flareite's own drags no longer use HTML5 drag
and drop: they're a pointer pressed and moved (`components/drag.ts`).

- **Starting.** A press on a tab, an activity bar button, or an explorer row becomes a drag once
  the pointer moves 4 px. The pointer is captured, so the drag continues over iframes (extension
  pages and previews) and outside the window. The click that ends a drag doesn't reach the
  element under it, so dropping a row doesn't open it.
- **Targets.** Elements register as drop targets (`useDropTarget`). As the pointer moves, the
  element under it (`elementFromPoint`) and its ancestors are asked, innermost first, whether they
  take the drag, as `dragover` events bubble. What the dock, the activity bar, and the explorer
  do with drops is unchanged.
- **Feedback.** A label with what's dragged follows the pointer (`DragGhost`, with ` +` when
  copying), and the existing previews show where it would land. Areas that scroll, such as the
  explorer and tab strips, scroll when the pointer is near their edge, as the browser did for
  HTML5 drags.
- **Ending.** Releasing drops. `Escape`, or the window losing focus, cancels. Holding `Ctrl`
  (`Option` on macOS) copies in the explorer, and can be pressed or released during the drag.

The alternative was keeping the HTML5 drags and, on Windows only, reading dropped files' paths
from WebView2 (`postMessageWithAdditionalObjects`, then `ICoreWebView2File::Path`). It would have
been a smaller change, but it's COM, which needs `unsafe` code (the workspace denies it), and
macOS and Linux would still have had no way to drop files.

### Files from other programs are drags like any other

The backend forwards Tauri's `WindowEvent::DragDrop` to the frontend as `fileDrag` events
(`file_drop.rs`): entering, moving, dropping, leaving, with the position and, on entering and
dropping, the files and folders. The frontend follows them with the same targets as its own drags,
so the same previews show:

- **The dock.** Files open where they're dropped: among a group's tabs, beside a group, or along
  an edge, the first there and the rest after it. A file already open moves there. In the middle
  of a group that isn't for documents (the side bar's views, say), they open where documents
  open, so they don't become side bar tabs. A folder becomes the open folder (the first one, if
  several are dropped).
- **The explorer.** Files and folders are copied into the folder under the pointer, as the
  explorer copies (` copy` added to a name that's taken).
- **Anywhere else** (the status bar, the activity bar): they open where documents open.

### Copying names the drop, not the files

The explorer's commands accept only paths inside the open folder ([ADR
0019](0019-explorer-file-operations.md)). Dropped files come from anywhere, so the backend
remembers the last drop and gives it an id. **`workspace_copy_dropped`** copies that drop's files
into a folder in the open folder. The frontend never names files outside the open folder to copy.

Opening needs nothing new: documents could always be opened from anywhere (Open File).

## Consequences

- **Pointer drags aren't HTML5 drags.** Dragging a tab out of the window does nothing (there was no
  dropping elsewhere before either). Text dragged inside the page, such as a selection dragged into
  an input, no longer drops, since Tauri's handler takes drops.
- **Touch** doesn't start drags; a press and move scrolls, as before.
- **Positions.** Tauri reports physical pixels from the webview's top left; dividing by
  `devicePixelRatio` gives CSS pixels, which includes the window's zoom.
- **Testing.** jsdom has no layout or `elementFromPoint`; tests say what's under the pointer
  (`test/drag.ts`). Unlike HTML5 drags driven over CDP, pointer drags driven over CDP go through
  the same input path as the mouse.
