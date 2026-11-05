# Troubleshooting

## Where Ferrite keeps things

| What                                  | Windows                                  | macOS                                              | Linux                                    |
| ------------------------------------- | ---------------------------------------- | -------------------------------------------------- | ---------------------------------------- |
| Settings, permissions, extensions     | `%APPDATA%\dev.ferrite.editor`           | `~/Library/Application Support/dev.ferrite.editor` | `~/.config/dev.ferrite.editor`           |
| Session state (`state.json`), backups | `%LOCALAPPDATA%\dev.ferrite.editor`      | `~/Library/Application Support/dev.ferrite.editor` | `~/.local/share/dev.ferrite.editor`      |
| Logs                                  | `%LOCALAPPDATA%\dev.ferrite.editor\logs` | `~/Library/Logs/dev.ferrite.editor`                | `~/.local/share/dev.ferrite.editor/logs` |

**Logs.** Logs roll over daily, and the last seven days are kept. For more detail, start Ferrite
with `FERRITE_LOG=debug`. To include messages from the user interface too, use
`FERRITE_LOG=debug,frontend=trace`.

**View: Output** (`Ctrl+Shift+U`) shows recent log messages inside Ferrite, one log at a time:
Ferrite's, an extension's (its **Log** button in the Extensions view opens it), or a language
server's, including everything the server prints. Extensions' and servers' debug messages show
there without changing `FERRITE_LOG`. **Open Log File** (beside the Output tab) opens the full
log.

**View: Problems** (`Ctrl+Shift+M`) lists what's wrong with Ferrite and extensions, including
the warnings and errors notifications showed before they disappeared, then the problems in
files. The status bar shows how many errors and warnings there are.

## Ferrite crashed or was closed unexpectedly

Your unsaved changes are backed up as you type. The next start restores them as unsaved tabs
and shows a warning listing them.

- **Check before saving.** A restored file opens with its text from disk, with your changes
  applied as one step. Undo shows what's on disk, and redo brings your changes back. If the file
  changed on disk in the meantime, saving asks before overwriting it.
- **Deleted files.** If a file was deleted since, its changes come back as an untitled document.
- **Backups that can't be restored** are kept in the `backups` folder (see the table above) and
  the warning says so. Each backup is a text file: two header lines, then your text.

Closing Ferrite normally, and choosing **Don't Save**, removes the backups. That's why they
don't come back after a normal close.

## Part of the window says it "stopped working because of an internal error"

A part of the window (the editor, a side bar view, or the status bar) failed to draw.

- **Try Again** redraws it.
- **Reload Window** restarts the whole interface. It's also in the command palette as
  **Developer: Reload Window**. Your open documents and unsaved changes are kept: they live in
  the backend, not the window.

Please report it with the log file's last lines: the error and where it happened are logged.

## Settings don't take effect

Ferrite applies `settings.toml` when you save it in Ferrite. If the file has a mistake (an
unknown key, a value out of range, invalid TOML), a notification names it, Problems lists it
until it's fixed, and the previous settings stay in effect. **Preferences: Open Settings** shows the mistake too, and can't change
settings until it's fixed. It lists every available setting with the value in effect.

To start over, delete `settings.toml`. **Preferences: Open Settings (TOML)** creates a fresh one.

## An extension misbehaves

- **A stopped extension.** If an extension fails, it's stopped and the Extensions view
  (`Ctrl+Shift+X`) shows why, with **Restart**. The editor and your documents aren't affected.
- **Is it slowing things down?** Each extension in the Extensions view shows how often it ran,
  for how long in total, its slowest call, and its memory. **Developer: Toggle Performance
  Overlay** shows the same as a table, next to the editor's own timings. Calls that take a
  quarter of their time limit are logged as `slow extension call`.
- **Changes skipped while busy.** An extension too slow to keep up skips document changes (the
  Extensions view says how many). It's then told to re-read the document.
- **Memory.** Each extension may use up to 128 MiB, and all extensions together 512 MiB. An
  extension that needs more is stopped with a message saying which limit it hit.
- **Turning it off.** **Disable** turns an extension off until you enable it again. To remove
  it, delete its folder from the `extensions` folder next to your settings.

## No syntax colors

Ferrite has no languages built in. Load the extension for the language (for example a
checkout of the official Rust extension, with **Extensions: Load Development Extension…**), and
check in the Extensions view that it isn't invalid or turned off. The status bar names the active file's
language once one claims it. If the language is there but the file stays uncolored, the log
says why under `syntax highlighting is unavailable` (a grammar that didn't load, or a query
error).

## Language features don't work

The status bar shows the language server's state for the active file. Servers come from
language extensions, which ask before running one: if you chose **Don't Allow**, turn
**Run language servers** on in the Extensions view.

- **Not found.** Install the server (for example `rustup component add rust-analyzer`), or point
  Ferrite at it: `[lsp.servers.rust-analyzer] command = "..."`.
- **Crashing.** A server that keeps exiting is restarted three times, then left stopped. The log
  has its error output. Changing its `[lsp]` settings, or reopening the folder, starts it again.

## Ferrite is slow or uses a lot of memory

- **See what's busy.** Open **Developer: Toggle Performance Overlay**: it shows input-to-paint
  latency, backend timings, background tasks, memory, open documents, and extensions.
- **Large files.** Files of any size open completely in memory, which takes about twice the file's
  size while it loads. Lines longer than 16 KiB are shown only up to that point, and in a file of
  more than about 1.6 million lines the ones after that can't be scrolled into view yet, though
  the cursor, editing, search, and saving still reach them.

## A file shows as binary, or in the wrong encoding

- **Binary.** A file with NUL bytes near its start opens in a tab that asks first. **Open Anyway**
  opens it as text; it's saved back byte for byte unless you change it.
- **Wrong encoding.** Encodings other than UTF-8 and UTF-16 are guessed. If the guess is wrong,
  click the encoding in the status bar and choose **Reopen with Encoding**. To convert the file,
  choose **Save with Encoding**.
- **"Can't be saved as …".** The file's encoding has no way to write a character you typed. Save
  it with an encoding that has it, such as UTF-8.
- **Extensions.** Disable extensions one at a time to rule them out. Benchmarks
  (`pnpm bench:app`) skip extensions unless you pass `--extensions`.

## Screen readers, contrast, and keyboard

- **Screen readers.** The editor's text is exposed through its input field. Moving the cursor
  reads the line, character, or word, and selecting reads the selection. Notifications are
  announced; errors are announced immediately.
- **Contrast themes.** Windows contrast themes are honored: selections, the cursor, and focused
  items use the system's colors.
- **Keyboard.** Context menus open with `Shift+F10` or the menu key. The side bar's width
  changes with the arrow keys on its edge. Every command is in the palette (`Ctrl+Shift+P`), and
  **Preferences: Keyboard Shortcuts** lists their keys.

## Starting from a clean slate

Close Ferrite, then remove `state.json` to forget the open folder, files, and layout. Remove
`extension-permissions.json` to be asked about extension permissions again. Your settings and
extensions are kept unless you remove those too.
