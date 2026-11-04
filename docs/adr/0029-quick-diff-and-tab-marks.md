# ADR 0029: Quick diff, partial staging, and marks on tabs

- **Status:** Accepted
- **Date:** 2026-11-03

## Context

The fourth stage of the Git extension brings VS Code's editor integration: lines changed from
the index marked beside the text as the user types, a change shown inline with buttons to stage
or revert just that change, staging or reverting the changes a selection touches (in an editor
or a diff tab), and tab titles colored by the file's status. The user chose markers that follow
typing (not saves), an inline view of a change when its marker is clicked, marks on tabs as a
general feature, and partial staging now.

## Decisions

### The editor compares; the extension names the original (API 1.9)

Comparing on every keystroke in WebAssembly would copy the whole text into the extension each
time, so the editor compares, with `ferrite-diff`, as in diff tabs. The extension only says what
a file is compared with: a manifest's `[quick_diff]` names a command that answers a file's
original with `set-original(path, uri)`. The original is an address the extension already
provides (ADR 0028), so Git's is `git::<path>`, the index version, for tracked files, and none
for untracked ones. The editor keeps the original's text (not as a document), and reads it again
when `documents::changed` names its address, as after staging. `originals-changed` asks again for
files' originals, when files start or stop being tracked.

Changes are computed in the background for each document version. A view carries those on its
lines (`changes`); while newer ones are computed, the last ones show, and a `QuickDiffChanged`
event makes the editors fetch views again. Whitespace counts, as the changes are reverted and
staged exactly.

### Reverting and applying move whole lines between the sides

`ferrite_diff::splice` turns chosen changes into edits: reverting replaces a change's lines in
the document with the original's, as one undoable edit; applying makes the original's new text,
which the extension's `apply` command receives whole (`to-apply`). Treating both texts as ending
with a line break makes changes at the end of a file come out right. Line breaks follow each side
(the index has `\n` where the file on disk may have `\r\n`).

Git stages by writing that text as a blob (`git hash-object -w --stdin --path=<path>`, so the
file's filters apply) and pointing the index entry at it (`git update-index --cacheinfo`), keeping
the entry's mode.

The changes a selection touches are those whose lines it covers; removed lines, which sit
between lines, are touched from the lines on either side. Commands act on the active editor:
**Revert Selected Ranges** is the editor's; **Stage Selected Ranges** is Git's, calling
`apply-selected-ranges`. In a diff tab both act between the tab's own two documents, and staging
is offered when the extension providing the original side declares `apply`.

### A change shown inline is a zone

Clicking a marker shows the change between the editor's lines, in a zone (ADR 0028) below its
last line: its position among the changes, buttons to apply (titled by `apply_title`), revert, go
to the previous or next change, and close, then the original lines, highlighted as the document's
language. Its lines are marked as inserted while it shows. An edit closes it, as its lines may
have moved. Alt+F3 and Shift+Alt+F3 show the next and previous change from the cursor.

### Marks on tabs

Tabs of documents, viewers, and diffs show the explorer marks of their file: the title in the
mark's color, the badge after it, and the tooltip added to the tab's. Any extension's marks
apply, not only Git's.

## Consequences

- Markers cost a diff per edit in the background, plus reading each original once per change to
  it. Files over 500,000 lines aren't marked.
- The editor knows nothing of Git: another version control extension can mark files and take
  staged changes the same way.
- Unstaging part of a staged change (HEAD against the index) isn't offered yet.
