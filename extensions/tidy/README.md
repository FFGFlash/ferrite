# Tidy

Tidy keeps whitespace tidy. It's an example of a Flareite extension with code, written in Rust
and built for WebAssembly.

## Features

- **Problems:** trailing whitespace, and lines longer than **Max Line Length** (100 by default),
  are shown as you type, with squiggles and in **View: Problems**.
- **Trim Trailing Whitespace** removes it from the active document, as one change you can undo.
- **Trim Trailing Whitespace in Files** is in the explorer's menu, for a file or a folder. In a
  folder it trims every text file except Markdown, where trailing spaces break lines. Files open
  in the editor change there, and are saved unless they have unsaved changes.
- **Count Words** counts the words in the selection, or the whole document.

## Settings

| Setting           | Default   | What it does                                          |
| ----------------- | --------- | ----------------------------------------------------- |
| `max_line_length` | `100`     | Longer lines are flagged; `0` turns the check off.    |
| `severity`        | `warning` | How problems show: error, warning, information, hint. |

## Permissions

Tidy reads and edits open documents to find and trim whitespace, and reads and writes the files
in the open folder for its explorer command. Without the file permissions, the rest still works.

See [`src/lib.rs`](src/lib.rs) for how it keeps its problems up to date without reading whole
documents on every change.
