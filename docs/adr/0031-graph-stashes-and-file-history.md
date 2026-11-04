# ADR 0031: History graph, stashes, and file history

- **Status:** Accepted
- **Date:** 2026-11-04

## Context

The sixth stage of the Git extension adds stashes, tags, cherry-picking and reverting commits,
the history graph, a file's history, cloning, and ignoring files. The user chose stashes as a
section of Source Control, a branch graph in VS Code's style, a timeline of the active file's
history, VS Code's clone flow (address, then folder, then an offer to open), and tags and commit
actions as commands and right-click entries.

Views could list items but not draw beside them; extensions couldn't tell which file the user is
editing, ask for a folder, or open one.

## Decisions

### Graph rows are data; the workbench draws them (API 1.11)

`views::set-graph(view, rows)` gives each listed item a row of a graph: its node, the node's
parents, and the lanes at the row's top and bottom, each an id (what it leads to) and a color.
That's VS Code's model (input and output swimlanes), so the workbench can draw any history-like
graph without knowing Git: lanes for the node end at it, other lanes carry on to the bottom lane
with the same id, and a lane leaves the node for each parent. The extension lays the lanes out
(`graph.rs`), going down the commits in topological order. Rows are SVG beside each item, one
width for the whole view; the rows of an opened commit's files carry the lanes straight on, as
the graph sits beside a tree, not a flat list.

Git lists the current branch with the branch it follows (`git log --topo-order HEAD
@{upstream}`), as VS Code's graph does by default, with branch and tag names in each commit's
description. Lanes take five colors by turns, which themes can change (`graph-lane-1` to `5`).

### The active file (API 1.11)

The workbench tells the host which file the editor in use shows: the current group's document,
viewer, or diff (its modified file). A tab that isn't an editor's, such as the Source Control
view, leaves it as it was, so a view beside the editor keeps following the file, as VS Code's
Timeline does. Extensions read it with `workbench::active-file()` and hear of changes through an
`on_active_file_changed` manifest command, like `on_folder_changed`.

Git's **File History** is a section of Source Control, not of the Explorer as VS Code's Timeline
is, because extensions contribute only to their own containers. It lists the file's uncommitted
changes, then up to 100 commits that changed it (`git log --follow`), each opening the diff of the
file as that commit changed it, under its name then.

### Folders: picking and opening (API 1.11)

`prompts::pick-folder` asks in the system's dialog and answers like a text prompt.
`workspace::open-folder` opens a folder as if the user had, so the trust question still comes.
Clone uses both. A clone runs as a remote operation (signing in through askpass, ADR 0030) in the
chosen folder, even with no folder open, and the offer to fetch in the background waits, so it
doesn't replace the offer to open the clone.

### Stashes and the rest are Git's own

- **Stashes** list `git stash list`; opening one lists its files against its first parent (the
  commit it was made on), reusing the commit files' items and diffs. They're listed again after
  every operation and when `.git/refs/stash` changes.
- **Tags** (with an optional message making an annotated tag), **Cherry-Pick**, **Revert**,
  **Checkout (Detached)**, and **Create Branch** at a commit are operations like the others.
- **Add to .gitignore** appends the files' paths, relative to the repository, to its
  `.gitignore`, skipping ones already there.

## Consequences

- Any extension can draw a graph beside a view, follow the active file, ask for a folder, or
  open one.
- Conflicts from popping a stash, cherry-picking, or reverting stop with Git's message; resolving
  them is the seventh stage.
- The graph covers the loaded commits (50 at a time); lanes leading to commits not loaded yet run
  to the bottom.
