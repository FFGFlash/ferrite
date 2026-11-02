# ADR 0018: Output and Problems

- **Status:** Accepted
- **Date:** 2026-11-01

## Context

When something goes wrong with Ferrite or an extension, the user has two places to look. One is a
notification, and warnings and errors stayed on screen until dismissed, so they piled up in the
corner. The other is the log file, which few users know about and which mixes every component
together. An extension's `log()` output and a language server's stderr only reach that file, and
debug-level messages don't reach it at all with the default filter. Some problems weren't shown
anywhere: an invalid `settings.toml` at startup was only logged.

## Decisions

### Output: recent log records in memory, by channel

A `tracing` layer (`logging.rs`) keeps recent records in memory, in channels:

- **`ferrite`:** Ferrite's own records.
- **`extension:<id>`:** records with an `extension` field (the extension's `log()` calls, and the
  host's records about it).
- **`server:<name>`:** records with a `server` field (a language server's starts, exits, and
  stderr).

Each channel keeps its newest 2,000 records, so a chatty extension can't push out Ferrite's. There
are at most 256 channels, and messages are cut at 8 KiB in memory. The full text stays in the log
file.

- **Levels.** The file and stderr keep their `FERRITE_LOG` filter (`info` by default). The memory
  layer has its own per-layer filter: `info` and above for Ferrite, and every level for extensions
  and servers. The filter looks only at a call site's metadata (its level and field names), so it
  is decided once per call site. Ferrite's own debug events stay disabled and cost nothing. This
  is how an extension's debug output and a server's stderr (logged at `debug`) appear in Output
  without a restart.
- **Polling, not events.** `logs_read(channel, after)` returns the channel list and the records
  newer than a sequence number. The Output view polls it once a second while it's shown. Pushing
  each record as an event would make logging emit events, and emitting can log, all from inside
  the subscriber.
- **The log file** is still the full record: **Open Log File** opens today's file in the editor.
- Extensions' `show-message` calls are now logged too, so the extension's channel has them after
  the notification is gone.

### Problems: what's wrong now, and what was reported

The Problems view lists two kinds of problems, grouped by source (Settings, Ferrite, or an
extension's name):

- **Current problems** come from state. They are an extension that stopped or couldn't be
  loaded, extension settings that were ignored, and a `settings.toml` that isn't in effect
  (`settings_problem`, checked at startup, after saves, and when settings change). They go away
  when fixed and can't be cleared.
- **Reported problems** are every warning or error a notification showed: failed actions,
  extensions' messages, and backend notices. Repeats are counted rather than listed again, at most
  200 are kept, and they stay until cleared (one at a time, or all of them).

The two don't duplicate each other: an extension's failure notification isn't recorded, because
the extension's status is listed while it's stopped. Each group offers what can be done: restart
the extension, open its settings, open `settings.toml`, or show its log. The status bar shows the
counts when there are any, and clicking them opens Problems.

Problems in files were added later; see [File problems](#file-problems-2026-11-01).

### Notifications disappear

Every notification now disappears after a while: information after 10 s, warnings and errors
after 20 s. None disappears while the pointer or focus is on it. Problems keeps the warnings and
errors. Only notifications marked sticky stay until answered. The permission prompt is sticky
because it's the only place that asks.

### Panel views

Problems (`Ctrl+Shift+M`) and Output (`Ctrl+Shift+U`) are views, so they can be docked anywhere.
Unlike side bar views, they open along the bottom with the terminals, aren't listed in the
activity bar unless docked in the side bar, and show and hide with the terminals
(`Ctrl+J`, `` Ctrl+` ``).

## Consequences

- Up to 2,000 records per channel are held in memory: about a few MB with many active channels.
- Language servers' stderr lines are now formatted and stored, which costs a little for chatty
  servers. They were formatted before only when `FERRITE_LOG` enabled `debug`.
- A warning is visible for 20 s unless the user looks at Problems or the status bar counts. That's
  the trade for not piling up popups.

## File problems (2026-11-01)

Problems now also lists problems in files, by file, after Ferrite's and extensions' own.

- **What's listed.** Every open document's diagnostics, from language servers and extensions,
  and the diagnostics language servers publish for files that aren't open. Before this,
  `on_diagnostics` dropped a publish for a file that wasn't open. Now it's kept by server and
  path, at the server's positions (its columns count the server's units, usually UTF-16, so a
  column can be slightly off for a UTF-8 server). Closing a document keeps its diagnostics the
  same way, so `cargo check` results (which rust-analyzer doesn't publish again) stay listed.
  Servers that only check open files clear them with an empty publish. Hints are left out, as
  in other editors. At most 10,000 are listed, with a count of the rest.
- **Fetched, not pushed.** `problems_list` assembles the list under the editor lock (then the
  language servers' lock, then the extension host's, in the usual order). The frontend fetches
  it at startup, when a document opens or closes, and 250 ms after `diagnosticsChanged` or the
  new `problemsChanged`, so a burst of publishes while typing fetches once.
- **The status bar** always shows the total errors and warnings (files, Ferrite, extensions) and
  opens Problems. It replaces the active document's counts, which are gone from the language
  status (and `ExtensionHost::problem_counts` with them).
- **Going to a problem** reveals it in its document, or opens its file at the position. Groups
  collapse (**Collapse All** beside the tab), and the arrow keys, Home, and End move between
  rows.
