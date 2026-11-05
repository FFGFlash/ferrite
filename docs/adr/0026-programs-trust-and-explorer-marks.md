# ADR 0026: Programs, Workspace Trust, and explorer marks

- **Status:** Accepted
- **Date:** 2026-11-03

## Context

The first goal was a Git extension that works like VS Code's built-in Git support, delivered in
stages. The first stage marks changed and untracked files in the explorer and shows the branch.
The workbench already dims files Git ignores. That stays in the workbench, since find in files,
Go to File, and `files::list-files` skip ignored files too.

Extensions couldn't do any of this. They had no process access, the `files` interface refuses
`.git`, they weren't told about changes on disk, and they had no way to mark explorer rows. The
user wanted general APIs rather than Git support in the workbench. A future Prettier extension
needs the same process API.

Running Git on a folder runs code from it. `core.fsmonitor`, `core.sshCommand`, and hooks name
programs. A `.git/config` in a downloaded archive can make a plain `git status` run anything.
Language servers have the same problem: rust-analyzer runs build scripts. VS Code guards against
this with Workspace Trust, and Flareite had nothing like it.

## Decisions

### Extensions run programs their manifest declares, in API 1.6

The new `processes` interface has `run` (waits, within the call's deadline), `spawn` (returns at
once, and runs a command named in the call when the program ends, during which `finished` returns
the outcome), and `kill`.

- **Declared, not named at run time.** `[[programs]]` lists each program by name. It is found on
  the `PATH`, as a path in the extension's folder, or as a path in the open folder (`workspace`,
  such as `node_modules/.bin/prettier`), and the last only in a trusted folder. Users can set a
  path in `[extensions.programs]`. The permission prompt lists the programs.
- **A new permission, `processes`**, is requested exactly when programs are declared, as with
  language servers.
- **Arguments, environment, input, and working folder are the extension's to choose.** A program
  the user lets an extension run can be told to do anything (`git -c core.sshCommand=…`), so
  restricting them would protect nothing. The permission is what protects, and its description
  says so.
- **Process groups.** `flareite-process` (factored out of `flareite-lsp`) starts each program in a
  job object or process group with no console window. Stopping one stops what it started, and
  everything an extension started stops with it.
- **Results by command, not by a new export.** Adding exports breaks components built against
  older versions ([ADR 0025](0025-webviews.md)), so a finished program, like a webview message,
  runs a command, queued even when the extension's queue is full.

### Extensions can watch files

`workspace::watch(path, command)` runs `command` with the changed paths, merged while the
extension is busy. A path inside the open folder needs `files-read`, and uses the editor's own
watcher, `.git` included. A path outside it (the `.git` folder of a repository the open folder
is inside) needs `processes`, since a program can read it anyway, and gets a watcher of its own.
`on_folder_changed` in the manifest runs when the open folder changes or the user changes whether
they trust it.

### Workspace Trust

The first time a folder opens, a dialog asks whether the user trusts its files. The user can
trust the folder that contains it instead, and with it every folder in it. Until they trust it,
the folder is in Restricted Mode, shown in the status bar. Language servers don't start, and
`processes` calls fail with `untrusted`. Everything else works, extensions included, and
`workspace::folder` tells them whether the folder is trusted. Closing the dialog without
answering leaves the folder restricted, and it asks again next session. Folders the user doesn't
trust stay that way, even inside a trusted one. `security.workspace_trust = false` trusts every
folder. With no folder open, nothing needs trust.

Every folder starts undecided, including folders opened before this version.

### Explorer marks, with colors themes can change

`explorer::set-file-marks` replaces an extension's marks. Each mark has a badge of one or two
characters, a color, a tooltip, and three flags. `faded` dims the row. `inherit` covers everything
inside a folder (an untracked folder). `propagate` colors the folders that contain the file,
where the highest `priority` wins. Marks need no permission.

Colors aren't hex values in the call. The manifest declares them under `[colors]`, with dark and
light defaults, as theme tokens (`flareite.git.modified`), which themes and `[workbench.colors]`
can set. Theme validation accepts any well-formed extension token, since a theme may color
extensions the user hasn't installed.

The explorer draws marks after the ignored color and before problems. A problem's color wins over
a mark's, and both badges show. A folder shows a problem's dot or a mark's dot, not both.

## Consequences

- An extension with `processes` can do anything the user can, in trusted folders. The permission
  prompt names its programs and says what that means; trust is the second gate.
- The Git extension (`extensions/git`) uses only these general APIs, and the workbench knows
  nothing of Git.
- Every existing folder asks once. That is the price of not trusting folders that were opened
  before trust existed.
- `run` blocks the extension's thread. Long operations (fetch, push) use `spawn`, and later
  stages will add progress for them.
