# ADR 0033: Blame, several repositories, and large repositories

- **Status:** Accepted
- **Date:** 2026-11-04

## Context

The last stage of the Git extension adds blame, several repositories, and speed in large
repositories. The user chose blame both after the cursor's line and in the status bar, as VS
Code shows them; support for several repositories (nested ones, submodules, and folders holding
several); and an `auto` mode for Git's speed-ups that the user can override. The README is the
user's guide; nothing else is written.

Extensions couldn't show text after a line, didn't hear when the cursor moved, and couldn't hide
a section of a container.

## Decisions

### Line annotations, the cursor's line, and hidden sections (API 1.13)

- **`workbench::set-line-annotations`** puts faint text after lines, with a tooltip. The host
  keeps annotations as it keeps code lenses (anchored to their lines, mapped through edits,
  dropped with the extension), and views carry those on their lines.
- **`on_selection_changed`** in the manifest runs a command when the active editor's cursor moves
  to another line or another document is in use, with that document and its primary selection
  as the command's context. The frontend reports the line after it settles (150 ms), not on
  every move within a line.
- **`views::set-visible`** shows or hides a view, and a view's `hidden = true` hides it until
  then, as VS Code's Repositories view shows only with more than one repository.

### Blame follows the cursor

Git annotates only the cursor's line, as VS Code does by default, and shows the same in a status
bar item, from a blame of the whole document read in the background (`git blame --porcelain
--contents -` with the document's text, so lines not committed say so). The blame is kept for
the document's version; when the cursor moves on a newer version it's read again, and what's
shown goes as soon as the document changes. Extensions have no clock, so ages ("3 weeks ago")
count from the time Git gives a new commit (`git var GIT_COMMITTER_IDENT`), and the user's own
commits (by `user.email`) are "You".

### Several repositories, one active

Every repository found has its status read in the background, for explorer marks (all of
them), change markers, conflicts, and the Repositories view. One is active, and the views and
the branch item show it, so the views stay as they were; choosing a repository, or opening a
file in one, makes it active. Commands act on the active one, and an operation runs in the
repository that was active when it was asked for.

Repositories are found as VS Code finds them by default: the one around the open folder, then
its untracked subfolders and checked-out submodules that are repositories of their own (Git's
status lists them as untracked folders); or, when the folder isn't in a repository, its top
subfolders. Ignored folders aren't looked in.

`git:` addresses name files relative to the base folder (the top repository, or the open folder
when it isn't in one), so a nested repository's file is `git:HEAD:sub/a.rs`: one address
scheme serves every repository, and the top repository's addresses are as before.

### Large repositories use Git's speed-ups, without changing their configuration

Git counts a repository's files once (`git ls-files`). With `performance = "auto"`, one with
more than 20,000 runs `git status` with `-c core.fsmonitor=true` (where Git has its own monitor:
Windows and macOS) and `-c core.untrackedCache=true`, unless its configuration sets them; `on`
uses them in every repository, overriding its configuration, and `off` never does. As command
options, they leave the repository's configuration alone.

## Consequences

- Any extension can annotate lines, follow the cursor's line, and hide sections.
- Repositories nested in ignored folders, or deeper than the first untracked folder, aren't
  found.
- The views show one repository at a time, not every repository's changes together as VS Code
  can.
- Blame isn't shown for documents of more than 50,000 lines.
