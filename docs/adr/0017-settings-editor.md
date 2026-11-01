# ADR 0017: Settings editor

- **Status:** Accepted
- **Date:** 2026-11-01

## Context

Settings are a validated `settings.toml`, edited as a document (ADR 0003). The template lists
every built-in setting, commented out, but only when the file is first created. Settings added
since then are missing from the file. Settings that extensions declare are never listed: users
had to read an extension's manifest to learn their keys, types, and ranges. Open Settings needs a
page that lists every setting with its value and lets the user change it, and the file stays
available for anything the page can't edit.

## Decisions

### The backend describes every setting

`settings_schema` returns each setting with its keys in the file (`["editor", "tab_size"]`), its
section, title, description, type, default, the value in effect, whether `settings.toml` sets it,
and a problem if the file's value isn't used. The file's path is returned too, and its parse error
if it's invalid.

- **Built-in settings** are a table in `settings/schema.rs`. Each entry reads its value from a
  `Settings`, so defaults come from `Settings::default()` and aren't written twice. A test checks
  every entry against the template.
- **Extension settings** come from `ExtensionHost::configurations()`: each manifest's
  `configuration`, with the resolved value and the reason an invalid user value was ignored.
  Titles are made from keys (`max_line_length` becomes "Max Line Length").
- **Choices** that depend on what's installed are filled in by the backend: color and icon themes
  from enabled extensions, and terminal profiles from detection. Built-in choice settings accept
  other values (a theme whose extension is off), which the page shows as "not available".
- **"Modified"** means the key is present in the file, not that the value differs from the
  default. Resetting removes the key.

### One setting is written at a time, through `toml_edit`

`settings_set(path, value)` checks the value's type against the setting, and an extension
setting against its declaration (`ConfigProperty::check`). It then sets or removes that key with
`toml_edit`, so the file keeps its comments, order, and formatting. A replaced value keeps its
trailing comment. Missing tables are created as implicit tables (`[extensions.settings."x"]`
without an empty `[extensions.settings]`), and inline tables are edited in place.

`settings::edit`, which theme and profile commands also use, now validates the whole result with
`Settings::parse` before writing. Before this change it wrote the file and then reported it
invalid. If the file is already invalid, the change is refused with the parser's message, and the
page shows the error and disables its controls until the file is fixed.

Changes made this way apply without the "Settings applied." notification (`announce: false` on
`settingsChanged`), because the user can see the change. Saving the file still announces it.

### Tables stay in the file

Theme color overrides, language servers, terminal profiles, and keybindings are maps of entries.
The page lists them with the names of the entries that are set, and links to the file. A form for
each would be four more editors, and these settings are rarely changed.

### A page, a new kind of dock item

The settings editor is `page:settings`, a new dock item kind for the workbench's own pages
(`app/pages.ts`). It opens where documents open, is saved with the layout, and can be docked like
any item. A view would have put it in the activity bar, and a document would have needed a fake
file. **Preferences: Open Settings** (`Ctrl+,`) opens the page; **Open Settings (TOML)** opens the
file. The page searches titles, keys, and descriptions, and `@ext:<id>` and `@modified` filter by
extension and by what the file sets. Each extension with settings has a **Settings** button in
the Extensions view, which opens the page filtered to it.

## Consequences

- Adding a built-in setting means adding it to `Settings`, the template, and the schema table. The
  test catches a schema entry missing from the template, but not the reverse.
- Text and number boxes write when Enter is pressed or focus leaves them, one write per change.
  Every write re-reads and applies the whole file.
- If `settings.toml` is open in the editor, a change from the page is a change on disk. It
  reloads the document if it's unchanged; otherwise the editor warns as for any other program's
  change.
