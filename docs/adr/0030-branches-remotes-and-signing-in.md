# ADR 0030: Branches, remotes, and signing in

- **Status:** Accepted
- **Date:** 2026-11-04

## Context

The fifth stage of the Git extension adds branches and remotes as VS Code has them: a branch
picker from the status bar, creating, renaming, deleting, and merging branches, publishing them,
fetching, pulling, pushing, and syncing with a status bar button, fetching in the background, and
Git's commands in an output channel. The user chose VS Code's picker, signing in within Flareite
rather than relying on credential helpers alone, background fetching off by default but offered,
a sync button that confirms the first time, and an output channel.

Two things were missing from the extension API: a way for a program to ask the user for a
password, and a clock to fetch every few minutes by.

## Decisions

### Programs sign in through the editor's executable

Git asks for credentials by running the program in `GIT_ASKPASS` with the question, reading the
answer from its output; SSH does the same with `SSH_ASKPASS`. A program the manifest declares
with `askpass = true` runs with both naming Flareite's own executable (`Embedder::askpass_program`)
and with an address and token in `FLAREITE_ASKPASS_*`. Run that way, the executable doesn't start
the editor: `askpass::client_main`, first in `main`, sends the question over a local TCP
connection to the host, which asks the user in the command palette and replies. Using the
editor's executable needs nothing installed beside it, which matters as nothing ships with
Flareite yet. A token per extension lets only its programs ask and names it; the answer goes to
the program, never to the extension's code. Questions about passwords, passphrases, PINs, and
tokens hide what's typed. Unanswered questions are withdrawn after five minutes.

Prompts could already answer only to an extension command; they can now answer a waiting thread
too (`Shared::ask_user`). This needs no WIT change: the manifest declares it.

Credential helpers still come first, as Git asks them before the askpass program, so Git
Credential Manager's own sign-in keeps working where it's set up.

### Timers (API 1.10)

Extensions have no clock. `timers::set-timer(delay, command)` runs one of their commands once
after 1 second to 1 day, on a thread that waits; `fired()` names the timer during it, and
`clear-timer` cancels. They're dropped when the extension stops. Settings change without the
extension hearing, so Git's background fetch keeps its timer running while a repository is open
and fetches only when `autofetch` is on: turning it on needs no restart.

### Git's side

- **The branch picker** lists Create New Branch, Create New Branch From, and Checkout Detached,
  then branches and tags, most recently committed first (`git for-each-ref`). A remote branch
  checks out as a local branch following it, or the local one if it exists. Spaces in new branch
  names become `-`.
- **Commands that reach a remote** run in the background without a time limit, one at a time with
  the others, and show in the sync button while they run. Sync is a pull, then a push only if the
  pull succeeded. A branch that follows none offers Publish (`git push -u`), asking which remote
  when there are several.
- **The sync button** (`↓` behind, `↑` ahead) replaces the counts on the branch item, which now
  opens the picker.
- **Background fetching** is offered once, after the user first reaches a remote, and remembered
  in the extension's storage.
- **Output.** Every Git command, and what a failed one said, goes to the extension's log, which
  the Output view shows as Git.
- The menu limit is 100 per extension, as a full Source Control menu needs more than 50.

## Consequences

- Any extension's declared program that uses `GIT_ASKPASS` or `SSH_ASKPASS` can sign in; other
  programs aren't affected.
- The askpass server listens on a local port while Flareite runs, after the first such program
  starts; only holders of a token are asked anything.
- Merging can stop with conflicts; resolving them is the seventh stage.
