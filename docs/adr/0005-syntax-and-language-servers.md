# ADR 0005: Syntax highlighting and language servers

- **Status:** Accepted
- **Date:** 2026-10-23

## Context

Milestone 5 asks for Tree-sitter syntax highlighting and basic Language Server
Protocol integration, enough for a useful programming experience in at least
one language. Both have to keep the M4 budget: a keystroke should still paint
in one frame, and nothing a server does (slow answers, floods of
notifications, crashes) may block editing. Rust owns document text, so parsing
and server sync belong in the backend.

## Decisions

### Tree-sitter in the backend, highlights in the view

Parsing and highlighting run in Rust (`flareite-syntax`), next to the text. The
frontend receives per-line spans (UTF-16 columns and token kinds) in the same
view response as the lines, so a keystroke still costs one round trip, and
the colors always match the text being painted.

Alternatives considered:

- **Tree-sitter compiled to WASM in the webview.** The webview would need the
  whole document to parse, which contradicts keeping text in Rust and costs a
  second copy of every large file.
- **A separate "tokens" request.** Simpler protocol, but a second round trip
  per keystroke and a window where new text shows old colors.

### Edit the tree first, reparse within a budget, fall back to a worker

The buffer's change log now records each edit as byte offsets plus row/column
points, which is exactly what `Tree::edit` needs (and what UTF-8 language
servers need). On each view, the tree is edited to the current version, then
an incremental reparse runs inline for at most 3 ms. Longer parses (first
parses, very large files) go to the scheduler over a snapshot, and the view
uses the edited old tree meanwhile, so text keeps its colors while typing.
Only one background parse per document runs at a time.

Alternatives considered:

- **Always parse in the background.** Every keystroke would show one frame of
  stale colors on the edited line, and each edit would cost a scheduler round
  trip and an extra view request.
- **Always parse inline.** First parses of large files take tens of
  milliseconds (22 ms for a 676-line Rust file in a release build, including
  compiling the highlight query on first use), too long to hold the editor
  lock.

### Send only changed highlights

Spans are sent only for lines whose spans changed, using the same
known-lines protocol as the text: the backend keeps a hash of the spans it
sent for each held line and replays line splices on those hashes. A keystroke
sends one line's spans, scrolling sends spans for newly visible lines, and a
change that recolors many lines (opening a block comment) sends exactly those.
A client and backend that disagree fall back to sending all visible spans.

### Language servers on their own threads, positions in UTF-8 when possible

`flareite-lsp` runs each server with a writer thread (notifications are queued,
so sending never blocks under the editor lock), a reader thread, and a stderr
logger. Requests are made from blocking threads with timeouts and
cancellation, never while holding a lock. The client offers the UTF-8
position encoding first: rust-analyzer accepts it, so positions and
incremental changes come straight from the edit log. UTF-16 servers get
positions converted through the rope, and their incremental changes are
computed by replaying edits on the last synced snapshot.

Alternatives considered:

- **An async LSP framework (`tower-lsp`/`async-lsp` style).** These are
  designed for writing servers or need an async runtime threaded through the
  app. Three plain threads per server are simple, bounded, and easy to test
  with a scripted fake server.
- **Always UTF-16.** Correct but needs a conversion for every position and
  change, and the LSP 3.17 negotiation exists to avoid exactly that.

### Sync on every view; diagnostics as anchored byte ranges

Every view first sends the document's pending edits to its server, so the
server is never more than one view behind, and any request made after a view
refers to text the server has. Diagnostics are interpreted against the synced
snapshot of the version they name and stored as byte ranges, then mapped
through later edits when shown. Late diagnostics therefore land on the right
text instead of drifting.

### Known servers by default, settings to change them

rust-analyzer, typescript-language-server, and pyright are configured by
default and used if installed; `[lsp]` in `settings.toml` can change their
commands, add servers, or disable them. Each server serves a workspace root:
the open folder if it contains the file, else the nearest root marker. A server
that exits is restarted on demand up to three times.

## Consequences

- Highlighting and server sync share the editor's single request path, so the
  M4 measurements still describe keystroke cost. In a release build,
  highlighting a viewport costs about 1 ms (2 ms p95) per view on a
  676-line Rust file, and keystroke-to-paint stays at one frame (p50 14.6 ms;
  see [`performance.md`](../performance.md)).
- Language servers dominate memory: rust-analyzer's process tree used about
  2 GB after indexing a small crate with the standard library, against about
  90 MB for Flareite itself. CPU contention while a server indexes can delay
  frames; the benchmark measures typing once the server is idle.
- Each grammar adds to binary size; adding languages is a code change until
  extensions can contribute grammars (M6). They can since
  [ADR 0013](0013-extension-languages.md).
- Find references, rename, code actions, formatting, and snippets are left for
  later; the client and sync layers don't need to change for them.
