# ADR 0027: Tree views, view containers, and prompts

- **Status:** Accepted
- **Date:** 2026-11-03

## Context

The second stage of the Git extension is a Source Control view like VS Code's. It has a commit
message box, staged and unstaged changes with buttons to stage, unstage, and discard them, and
the commits of the current branch, each opening to list its files. Discarding asks first, and
committing with nothing staged asks whether to commit everything.

Side bar views were plain trees, with one command per item ([ADR 0007](0007-workbench-contributions.md)).
They had no icons, buttons, menus, text box, or way to load children on demand. Extensions also
couldn't ask the user anything, delete a file, open one, or remember a choice in their
settings. VS Code has a dedicated source control API. The user chose general view features
instead, keeping the workbench unaware of Git and useful to any extension. The history graph
may later be a webview in the side bar.

## Decisions

### Views keep their shape and gain features, in API 1.7

A new `views` interface works on the views a manifest declares. `set-items` takes richer items
than `workbench::set-view-items`, which stays as it was for older extensions. Changing a record
in place would break components built against older versions.

- **Items** have an icon (a named icon, or the icon theme's for a file or folder name), a mark
  (badge, a declared color, strikethrough, faded), a context for menus, and an expansion state.
- **Children on demand.** An item that can be opened but has none listed runs the view's
  `on_expand` command when opened, which calls `set-children`. The command learns the item from
  `views::expanding()`, like webview messages and finished programs, rather than through a new
  export. Children are asked for again only when new content still lacks them.
- **Menus.** Two menu locations, `view/title` and `view/item`, use the existing `[[menus]]`
  table: `inline` makes an entry a button (which needs a named icon), and `contexts` picks the
  items it's for. `when` tests flags the extension sets (`set-flags`), a deliberately small form
  of VS Code's when-clauses: one flag, or its negation.
- **Selection.** Items can be multi-selected. A menu command learns the selected items'
  arguments from `views::selection()`: all of them if the clicked item is selected, else the
  clicked one.
- **A text box, a welcome message, and a badge** per view. The text box's text is kept by the
  host as the user types, without telling the extension, which reads it when it needs it.
  Ctrl+Enter sends the latest text before running the box's command.

### View containers hold views as sections

`[[view_containers]]` declares an activity bar entry. Views with `container` are its sections:
each has a header with its title, badge, and buttons, collapses, and scrolls on its own. Open
sections share the height equally; resizing them is left for later. A container's badge adds
up its sections'. Showing a section shows its container.

### Prompts

A `prompts` interface asks the user to pick from a list or type a line of text (both in the
command palette, as built-in quick picks do) or confirm in a dialog. Answers run a command, as
everything asynchronous in the API does. One prompt shows at a time: a new one cancels the one
showing, closing one answers `cancelled`, and a prompt goes away when its extension stops.

The confirmation dialog shares `ModalDialog` with the trust dialog. The main button comes last,
in red when `destructive`, and focus starts on the dialog itself, so Enter never confirms by
accident.

### Files, opening, and settings

- `files::delete(path, to-trash)` deletes inside the open folder with `files-write`, moving to
  the system's trash when asked, and fails without deleting if the trash can't take it. The Git
  extension's `discard_untracked` setting chooses, and defaults to the trash, which makes
  discarding an untracked file undoable.
- `workspace::open-file(path)` opens a file as the user would, in its default viewer if it has
  one.
- `host::set-configuration(key, value)` writes one of the extension's own settings, checked
  against its declaration, to `settings.toml`. It takes effect at once, before the file is read
  again. Git saves "Always" and "Never" from its smart commit prompt this way.

### The Git extension

The Source Control container has a Changes section (Merge Changes, Staged Changes, and Changes
groups, as a list or a tree of folders) and a Commits section (50 at a time, with Load More).
Commands that change the repository run one at a time, each followed by a status. A commit's
details open in a preview tab from its right-click menu.

Status now lists untracked files one by one (`--untracked-files=all`), since the view lists
them, and covers the whole repository, so changes staged outside the open folder are visible
before they're committed.

## Consequences

- Any extension can build views like Git's. A TODO list can have buttons, and a test runner can
  load results when a suite is opened.
- `when` is a single flag. Richer conditions would need a small expression language; none is
  needed yet.
- Sections can't be resized, and a collapsed section's state isn't saved between sessions.
- Clicking a changed file opens it. When the diff editor arrives (stage 3), it opens the
  changes instead.
