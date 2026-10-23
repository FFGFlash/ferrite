# ADR 0002: Text model and editor protocol

- **Status:** Accepted
- **Date:** 2026-10-23

## Context

Milestone 2 makes Ferrite a working text editor. The design brief requires Rust to own the
authoritative document, the frontend not to keep full copies of documents, correct Unicode
handling, unambiguous position APIs, efficient undo with a retention policy, and safe saving that
never silently discards edits. These choices shape every later subsystem (syntax, LSP,
extensions), so they're recorded here.

## Decisions

### Ropey 1.6, lines broken by LF, CRLF, and CR only

Ropey 2.0 is still in beta, so we use Ropey 1.6.1 with default features off and `cr_lines` +
`simd` on. The default `unicode_lines` feature would also break lines at U+2028, NEL, and others.
That disagrees with LSP and with how compilers count lines, so it would produce wrong line numbers
for diagnostics.

### Byte offsets internally, UTF-16 at the frontend boundary

All Rust APIs use UTF-8 byte offsets (`usize`). Ropey is char-indexed internally, but byte
offsets match Rust strings, tree-sitter (M5), and `&str` slicing without conversion. The frontend
receives line + UTF-16 column positions (`Utf16Position`), because those are JavaScript string
indices into the rendered line. LSP also defaults to UTF-16, so the same conversion will serve
M5. Conversions are O(log n) through Ropey's indexes. We considered newtype wrappers for byte
offsets, but they would add noise to nearly every line of editing code for little safety, since
the other units already have their own types.

### Grapheme-aware movement, `unicode-segmentation`

Cursor movement and deletion step over extended grapheme clusters (`unicode-segmentation`'s
`GraphemeCursor`, driven across rope chunks). This keeps the cursor out of combining sequences,
emoji ZWJ sequences, and CRLF pairs. Every boundary function first snaps its input to a
character boundary, so no caller can make it panic.

### Change sets with inverses; versions never reused

Edits are atomic `ChangeSet`s in pre-edit coordinates, validated before any mutation. Applying
one returns its inverse, so undo needs no diffing. Every content change, including undo and redo,
produces a new version, so a version identifies exactly one text state. Extensions (M6) will
submit edits against a version and be rejected if it's stale.

### History groups, state ids for dirty tracking

Undo stores forward/inverse pairs grouped by edit kind, bounded by group count and stored bytes.
Dirty state compares history state ids instead of versions, so undoing back to the saved state is
clean while versions keep increasing.

### The core does no I/O

`ferrite-core` receives loaded text and produces save points. `ferrite-workspace` does the I/O,
and `ferrite-app` coordinates it, including dialogs. This keeps the core deterministic and
testable, and lets saving run on a blocking thread without holding the editor lock. The core
depends on `ferrite-workspace` only for the `FileStamp` type it stores per document.

### Operation protocol with viewport views

The frontend sends editing _operations_ ("insert `x`", "move word left", "set cursor at line 3,
column 5") and gets back a _view_ of its viewport. We considered three alternatives:

1. **Frontend owns text, sends diffs** (a textarea or CodeMirror-style model). This contradicts
   the brief's requirement that Rust own the authoritative document, and it duplicates every
   document in the webview.
2. **Rust pushes events for every change.** That needs change-tracking state per view and still
   needs pull-based fetching for scrolling.
3. **Operations in, viewport views out.** This was chosen. It's one request/response per batch,
   the payload is bounded by viewport size rather than document size, and the frontend stays
   stateless about text.

Requests are serialized per editor, and operations queued during a request are batched into the
next one. Sending whole viewport views instead of deltas costs a few KB per keystroke. M4
measures this cost and can switch to line-level deltas behind the same session API if needed.

### Rendering: DOM lines, measured overlays

The frontend renders only the viewport's lines as plain DOM text and measures carets and selections
with `Range` and `caretPositionFromPoint`. This uses the browser's text shaping (ligatures,
emoji, bidi, tabs) instead of a custom shaping engine, which the brief advises against. Input
goes through a hidden textarea so IME, paste, and platform keyboard behavior keep working.

### Native dialogs and clipboard from Rust

Open/save dialogs, unsaved-changes prompts, and clipboard writes happen in Rust through the
official Tauri plugins. The webview needs no dialog or clipboard permissions, and copied text
never crosses IPC.

### Safe saving

Save by writing a temporary file, `fsync`, and renaming it over the target. Before writing, check
the target's stamp (modification time + length) against the one recorded at load or last save;
on mismatch, ask before overwriting.

## Consequences

- Each keystroke is one IPC round trip; typing latency depends on IPC latency. It felt immediate
  in manual testing on Windows, but it hasn't been measured; that's M4.
- Lines are rendered whole, so pathological long lines need special handling (M4).
- Positions from the frontend are interpreted against the document at the moment each operation
  applies. A click that's batched behind typing could land slightly off, which is harmless
  because positions are clamped. Extension edits (M6) will use explicit versions instead.
- Opening a non-UTF-8 file fails with a clear error. Other encodings can be added later in
  `ferrite-workspace` without touching the core.
