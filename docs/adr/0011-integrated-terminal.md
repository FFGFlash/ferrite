# ADR 0011: Integrated terminal

- **Status:** Accepted; amended 2026-10-30
- **Date:** 2026-10-25

Since [ADR 0012](0012-docking.md), terminals are dock items rather than tabs in a panel below the
editor: each terminal is a tab that can be docked anywhere.

## Context

Users want a terminal inside the editor, as in VS Code: shells in a panel below the editor,
started in the workspace folder, several at once.

A terminal has two halves:

- **A pseudoterminal.** The shell must think it's talking to a terminal (line editing,
  colors, full-screen programs), which takes a pty on Unix and ConPTY on Windows.
- **An emulator.** It interprets the escape sequences the shell prints, and draws the screen,
  scrollback, cursor, and selection.

Output can be far faster than drawing (`cat` of a large file). The app must stay responsive,
and the shell's memory and the webview's must stay bounded.

## Decisions

### Pseudoterminals through `portable-pty`, in a new crate

`ferrite-terminal` runs each shell with `portable-pty` (from WezTerm, MIT, maintained), which
covers ConPTY and Unix ptys behind one interface. The crate has no Tauri dependency. Like
`ferrite-lsp`, it runs three threads per terminal:

- **A writer.** It drains a bounded queue of input (64 writes, at most 1 MiB each), so a
  program that isn't reading its input never blocks the caller.
- **A reader.** It decodes output as UTF-8 (holding back a character split across reads;
  invalid bytes become U+FFFD) and passes it to the attached sink.
- **A waiter.** When the shell exits, it closes the pseudoterminal and reports the exit code
  after the last output.
  - On Windows, the reader sees the end of output only once the pseudoconsole is closed.
  - A background program can hold a Unix pty open, so the exit is reported after at most a
    second even if output hasn't ended.

Alternatives considered:

- **`alacritty_terminal`.** It includes a pty layer, but it comes with a full emulator we
  wouldn't use (see below).
- **Separate crates per platform** (`conpty`, `pty-process`). That means two interfaces and more
  platform code.

### Emulation in the webview with xterm.js

The frontend uses xterm.js 6 (MIT), the emulator VS Code uses, with its fit addon. It handles
escape sequences, the alternate screen, mouse reporting, IME, bracketed paste, selection, and a
screen reader mode. The backend only moves text.

Alternatives considered:

- **Emulating in Rust and sending screens.** For example `alacritty_terminal` plus incremental
  row updates, like the editor's views. That would keep the scrollback in Rust, but every
  emulator feature listed above would have to be rebuilt in the frontend, and each output burst
  would cost a screen diff instead of a string.
- **Writing an emulator.** Out of proportion to the feature.

xterm.js is about as large as the rest of the frontend (335 kB minified). It loads with the
first terminal, as a separate chunk, so startup doesn't pay for it.

### Output as text through a channel per terminal

`terminal_create` and `terminal_attach` take a Tauri `Channel`. Output (`output`), replayed
output (`replay`), and the exit (`exited`) all go through it, in order. A channel per terminal
keeps the exit behind the last output, which separate event streams wouldn't guarantee.
Commands (`terminal_write`, `terminal_resize`, `terminal_acknowledge`, `terminal_kill`) are
synchronous and never wait on the shell.

### Flow control by acknowledgement

The frontend acknowledges output after xterm.js has parsed it, in steps of 50,000 UTF-16 code
units. The backend stops reading when 500,000 units are unacknowledged and resumes below
100,000. The pty's buffer then fills and the program blocks on its writes, so a flood can't
queue unbounded messages in the webview. While no frontend is attached, output isn't counted.

### Terminals outlive the webview

Each terminal keeps its last 256 KiB of output (trimmed at a line break). After **Reload
Window**, the frontend lists the running terminals and attaches to them, and the output kept so
far is replayed first.

- **Stale replies dropped.** The replay is marked, and the frontend drops what xterm.js answers
  to it. Otherwise old queries, such as ConPTY's cursor position request at startup, would be
  answered again into the shell.
- **One attach per terminal.** Restoring twice (React runs effects twice in development) skips
  terminals that are already attached, because a second attach would take the first one's
  channel.

### Shells and environment

- **Default shell.** On Windows, PowerShell 7 (`pwsh`) if it's on the `PATH`, otherwise Windows
  PowerShell. On macOS, `$SHELL` as a login shell, so profile scripts set up the `PATH` for an
  app started from the Dock. Elsewhere, `$SHELL` or `/bin/sh`. `terminal.shell` and
  `terminal.args` override it.
- **Working directory.** The workspace folder, or the home folder.
- **Environment.** `COLORTERM=truecolor`, `TERM_PROGRAM=ferrite`, `TERM_PROGRAM_VERSION`, and,
  outside Windows, `TERM=xterm-256color`.

### Keys belong to the shell

While a terminal has focus, only commands marked `terminal` in the registry run from the
keyboard: the palette, Go to File, side bar views, editor switching, settings, and the
terminal's own commands. Every other key goes to the shell (Ctrl+W deletes a word, Ctrl+P
recalls history). This is VS Code's "commands to skip shell" list, as a flag on the command.

Clipboard keys follow each platform's terminal conventions, and the browser's clipboard events do
the work, so no clipboard permission is needed:

- **Windows.** Ctrl+V pastes. Ctrl+C copies when text is selected, and interrupts otherwise.
- **Linux.** Ctrl+Shift+C and Ctrl+Shift+V copy and paste.
- **macOS.** Cmd+C and Cmd+V.

Keybindings with Shift and punctuation (`` ctrl+shift+` ``) now also match by the physical key,
because the browser reports the shifted character (`~`).

### Colors are theme tokens

The terminal's background, foreground, cursor, and selection follow the editor's tokens, and the
16 ANSI colors are tokens of their own (`terminal-ansi-*`), so themes and `workbench.colors`
can change them. xterm.js enforces a 4.5:1 minimum contrast, which keeps program-chosen colors
readable on either base theme.

### Resizing on Windows (2026-10-30)

Resizing a terminal could duplicate lines. Two causes, found by numbering 60–80 output lines,
resizing, and reading the whole buffer back:

- **Height.** By default, xterm.js pulls lines from the scrollback into a taller viewport, while
  ConPTY repaints a taller screen with empty rows at the bottom. xterm.js's `windowsPty` option
  (`{ backend: 'conpty', buildNumber }`) makes it leave new rows empty, as ConPTY expects. The
  backend reports the Windows build (`terminal_platform`, from the `windows-version` crate Tauri
  already uses).
- **Width.** Both sides rewrap long lines: xterm.js reflows its buffer, and the inbox ConPTY
  reflows and repaints its screen. Near the top of the screen they disagree, and lines are
  duplicated (many small resizes) or lost (one large one). Turning xterm.js's reflow off loses
  lines instead. This can't be fixed from the emulator: it's how the ConPTY in Windows behaves.

The ConPTY in Windows Terminal 1.22 and later doesn't repaint on resize. Microsoft publishes it
for applications to ship (`conpty.dll` and `OpenConsole.exe`, NuGet package
`Microsoft.Windows.Console.ConPTY`, MIT), and WezTerm and VS Code ship it for this reason.
Ferrite ships it too:

- **Downloaded by the build.** `src-tauri/build.rs` downloads a pinned version from nuget.org
  once, checks the package against its SHA-256, and unpacks the files for the target
  architecture into `src-tauri/conpty/` (ignored by git). It uses the `curl` and `tar` that come
  with Windows. A build without network access fails until the files are there.
- **Installed beside the executable.** `tauri.windows.conf.json` lists both files as resources,
  so tauri-build copies them next to the executable for `cargo run` and `tauri dev`, and the
  installers put them beside it. `portable-pty` loads a `conpty.dll` found there in preference to
  the one in Windows, and it starts the `OpenConsole.exe` beside it. They add about 1.2 MB.
- **Measured.** With it, the same tests keep every line in order: 60 wrapped lines through many
  small and several large width changes, and 80 lines through height changes. With the ConPTY in
  Windows, the width tests lost rows 33–41 (large changes) or repeated rows (small ones).
- **Updating.** Change the version and its hash in `build.rs` together.

## Consequences

- **Throughput.** End to end, `type` of a 2.7 MB file takes 2.9 s, against 2.0 s through
  ConPTY with no frontend. 200,000 lines from a PowerShell pipeline take 39 s, against 32 s.
  The pipeline itself takes 1.1 s writing nowhere, so ConPTY and the console host dominate. The
  window stays responsive throughout (worst frame wait 37 ms). xterm.js's WebGL renderer could
  narrow the remaining gap.
- **Not persisted across restarts.** Terminals survive a window reload, not quitting the app.
  Quitting ends every shell and the programs it started.
- **Replay is approximate.** Trimming and reattaching at a different size can garble full-screen
  programs until they redraw. Nothing is lost for line-oriented output.
- **Windows builds download ConPTY** on the first build, and need network access for it (see
  above).
- **Not yet supported.** Clickable links and file paths, shell integration
  (command marks, cwd tracking), per-terminal profiles, and an API for extensions to create
  terminals.
- **Screen readers.** Output is announced only with `terminal.screen_reader = true`, because
  xterm.js's screen reader mode costs time on every update.
