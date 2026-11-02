# ADR 0019: Explorer file operations and the files API

- **Status:** Accepted
- **Date:** 2026-11-02

## Context

The explorer could create and open files and folders, nothing else. Renaming, moving, or
deleting one meant leaving Ferrite, and an open document then pointed at a path that no longer
existed. Extensions could add items to the explorer's menu for files, but not folders, and an
item got only a path it couldn't use: an extension has no file access, so a formatter's
**Format All** on a folder was impossible.

## Decisions

### Operations

Rename (`F2`), delete (`Delete`; permanently with `Shift+Delete`), cut, copy, and paste, drag and
drop (move; copy with `Ctrl`, or `Alt` on macOS), copy path and relative path, and reveal in the
system's file manager. They're in the context menu, on the tree's keys, and in the palette (on
the focused row).

- **Single selection.** Each acts on one file or folder. The backend commands take lists, so
  selecting several rows can come later without changing them.
- **Nothing is replaced.** A rename or move onto a taken name fails with a message. A copy onto
  a taken name becomes `name copy.ext`, as in VS Code, so pasting beside the original
  duplicates it. A case-only rename (`readme.md` to `README.md`) works where names ignore case.
- **Deleting goes to the trash** (the `trash` crate), after a native confirmation. What can't
  go there (a network drive, say) can be deleted permanently after asking again. There's no
  undo for file operations: the trash is the way back.
- **Inside the open folder.** The backend refuses paths outside it, the folder itself (except
  as a destination or to reveal), and paths with `..`.
- **Drag and drop** stays inside the explorer. Dragging files in from the system isn't handled
  yet.

### Open documents follow their files

`Editor::follow_move` points documents of a renamed or moved file, or of files in a moved
folder, at their new paths. Language servers see the change on the next sync: the document is
closed under the old path and opened under the new one (so a rename to another extension
changes its server). Extensions get the same: `document-closed`, then `document-opened` with the
new path and language. Their diagnostics for it are dropped until they publish again.

Documents of deleted files stay open, as they do when another program deletes a file. Saving
one writes the file again.

### Extension menus for folders and file names

Menu items gain `targets` (`files`, `folders`, or both; files by default, as before) for the
explorer, and `patterns` (file name globs: `*`, `?`, `{a,b}`, ignoring case) for both menus.
The manifest is checked when loading, and the frontend matches names. Patterns match names, not
paths, which covers "files of this type" without a `when` expression language.

### API 1.3: the `files` interface

Extensions can list, read, and write files in the open folder, with two new permissions,
`files_read` and `files_write`.

- **A new interface, nothing changed.** Adding cases to `api-error` or `permission` would change
  every function that uses them, and components built against 1.0 to 1.2 wouldn't link. So
  `files` has its own `file-error`, and the new permissions aren't in `activation.granted`: a
  call that lacks one fails with `permission-denied` naming it. A test loads a 1.2 build of Tidy.
- **Scope.** Only the open folder. Paths are resolved (links and `..`) before checking, so a
  link can't lead outside. `files.exclude` applies, and `.git` is always refused: writing hooks
  or Git's config there would make Git run an extension's code.
- **Open files go through the editor.** Reading an open file returns its document's text,
  unsaved changes included. Writing one changes the document as one undoable change, and saves
  it only if it had no unsaved changes (without asking, and never over a file changed on disk).
  A file the user is editing keeps their changes theirs to save, and an extension never works
  on an older copy than the user sees.
- **Line endings and encoding** are kept: written text gets the file's line ending, and a
  byte-order mark stays. Files are UTF-8, as everywhere in Ferrite.
- **Listing** is like Go to File: Git-ignored and excluded files are left out, up to 10,000.
- **Limits.** 16 MiB per file. Commands keep their 10 s deadline, which fits thousands of small
  files.

Tidy shows it: **Trim Trailing Whitespace in Files**, for files and folders.

## Consequences

- An extension with `files_write` can change any file in the open folder. The permission prompt
  says so, and the user can deny it and keep the rest.
- `Embedder` has two new methods with defaults (`open_folder`, `save`), so other embedders,
  like the tests', keep compiling.
- Path normalization moved to `ferrite-workspace` (`normalize_path`), so the extension host
  compares paths as the app does.
- Reveal shells out to `explorer` or `open` rather than adding `tauri-plugin-opener`, which would
  have brought about 30 crates (a D-Bus stack) for one function.
