# ADR 0023: Any file size, text encodings, binary files, and viewers

- **Status:** Accepted; amended 2026-11-02
- **Date:** 2026-11-02

Since [ADR 0025](0025-webviews.md), a viewer can be a webview, whose page loads its file itself,
and the Images example is one: its page draws the picture with a choice of background and size.

## Context

Flareite opened only UTF-8 files of up to 256 MiB ([ADR 0004](0004-performance-foundations.md)).
Anything else failed with an error: a Latin-1 or Shift JIS text file, a 300 MB log, and an image
someone clicked in the explorer. Other editors open text in other encodings, and VS Code shows a
binary file in a tab that asks before opening it as text. Images are the most common files people
click that aren't text, and showing them is the kind of thing an extension should be able to add
for a file type, rather than the workbench knowing about each format.

## Decisions

### No size limit

The 256 MiB limit goes, for documents and for crash-recovery backups. A file is still read whole
into memory (about twice its size while it loads), off the main thread. Find counts, backups, and
views were already built for large documents. The extension API keeps its own limits (16 MiB per
file read or written), which bound an extension's memory, not the editor's.

### Encodings, detected and kept

A document has a `TextEncoding`: an `encoding_rs` encoding and whether the file has a byte order
mark. It's detected on load and used on save, so text the user didn't edit is written back byte
for byte.

- **Detection** (`flareite-workspace/src/encoding.rs`): a byte order mark (UTF-8, UTF-16), then
  NUL bytes in the first 8 KiB, which mean binary unless every other byte is NUL (UTF-16 without
  a mark, as VS Code detects it), then valid UTF-8, then the legacy encoding `chardetng` guesses
  from the first MiB, and finally Windows-1252.
- **Never lossy.** A guess is used only if the file decodes in it without errors and encodes back
  to the same bytes. Shift JIS and ISO-2022-JP don't always (several byte sequences for one
  character, escape sequences). Windows-1252 always does: `encoding_rs` maps each of its 256
  bytes to a character of its own, so it's the fallback for anything. An encoding the user
  chooses must meet the same tests, or the file isn't reopened (`InvalidText`, `LossyEncoding`).
- **Saving** encodes as it writes. A character the encoding lacks fails the save
  (`Unencodable`), before the file is replaced, rather than writing a replacement.
- **Changing it.** The status bar shows the encoding; clicking it offers **Reopen with
  Encoding** (a clean document only; an undoable reload) and **Save with Encoding**. A reload
  after an external change reads the file in the document's encoding, and backups record it.

Alternatives considered:

- **Decode lossily, as VS Code does by default** (invalid bytes become U+FFFD). Saving then
  silently changes the file, which ADR 0004's "never silent changes" rules out.
- **No guessing**, only UTF-8, UTF-16, and Windows-1252. Simpler, but Cyrillic, Japanese, and
  Chinese files would open as mojibake. `chardetng` is Firefox's detector, small, and made for
  `encoding_rs`.
- **A setting for the default encoding.** Possible later; detection plus Reopen with Encoding
  covers the cases seen so far.

### Binary files ask first

A file that looks binary isn't opened as a document. It gets a tab of its own saying so, with
**Open Anyway** (as text: UTF-8 if valid, else byte for byte as Windows-1252, so saving an
unchanged file rewrites the same bytes), and a button for each viewer that can show it.

### Viewers: extensions show file types, in API 1.4

A `[[viewers]]` contribution declares a viewer: a title, file name patterns (as menu items have),
the command that fills it in, and a priority. `default` viewers open matching files; `option`
ones only when chosen with **Reopen Editor With** (the Images example's SVG viewer: SVG is text
people edit).

- **Content is preview content** ([ADR 0016](0016-previews.md)): the same typed nodes, checked
  and drawn the same way, so an extension still supplies no markup. Pictures and links resolve
  from the file's folder, so a picture of the file itself is its name.
- **The command is the entry point.** The workbench runs the viewer's command with the file's
  path as the argument (`command-argument`) when the tab shows, when the file changes on disk,
  and when the extension starts again. Adding an export would break older components (as ADR
  0016 found for previews), and explorer menus already pass paths this way.
- **A new interface, `viewers`**, has `set-viewer-content` and `read-viewed-file`. Reading needs
  no permission: a viewer reads only files open in it, which the user chose to show there, in
  ranges of at most 16 MiB (a header, or a page of a large file).
- **Tabs belong to the workbench.** A viewer tab is a dock item (`viewer:<viewer id>:<path>`),
  saved with the layout like documents. Its content lives in a host preview record that stays
  while the tab is open, even when the extension stops: only the content goes, and the host runs
  the command again when the extension starts. Previews, by contrast, close with their extension.
- **Opening a file**: a document already open for it wins, then a tab it already has, then a
  default viewer, then text, then the binary tab. Files named on the command line that turn out
  binary are handed to the workbench (`openFile`) to go the same way.

Alternatives considered:

- **Built-in image viewing.** Less code, but every other format would need the workbench too.
  ADR 0014 moved languages into extensions for the same reason.
- **Webviews (HTML from the extension).** Rejected for previews in ADR 0007 and 0016; the same
  reasons hold.
- **A new preview element for images.** Not needed: the workbench styles viewer content (a
  picture centered on a checkerboard), and `image` nodes already exist.

### The Images example

`extensions/images` declares the two viewers. Its command reads the file's first 64 KiB (more for
a JPEG whose size comes after large metadata), works out the format and size in pixels from the
header (PNG and APNG, JPEG, GIF, WebP, BMP, ICO and CUR, AVIF, SVG), and shows the picture with a
line such as `PNG · 1920 × 1080 pixels · 8-bit RGBA · 245.3 KiB`. Pictures may now also be BMP,
ICO, or AVIF, and up to 64 MiB (from 16), in viewers and previews alike.

## Consequences

- **Memory.** A file takes its size in memory, and about twice that while loading. Opening a
  multi-gigabyte file can exhaust memory; nothing stops it.
- **Very long files can't be scrolled to the end.** The editor's scroll area is one element as
  tall as all its lines, and WebView2 caps an element at about 33 million pixels: about 1.6
  million lines at the default line height. Lines beyond are reachable with the cursor, editing,
  and search, but not shown. The 256 MiB limit hid this only partly (a file under it could have
  more lines); scaled scrolling is the fix, and is left for later.
- **Extensions see more files as text.** The `files` interface reads and writes non-UTF-8 text in
  its own encoding; only binary files are `not-text`.
- **Pictures travel as `data:` URLs**, so a 64 MiB picture is about 85 MiB of JSON once. Streaming
  them through a protocol handler would remove that cost and the limit.
- **New dependencies:** `encoding_rs` (already in the tree through other crates) and `chardetng`.
