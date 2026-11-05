# ADR 0034: Official extensions in their own repositories

- **Status:** Accepted
- **Date:** 2026-11-05
- **Amends:** [ADR 0014](0014-languages-only-from-extensions.md) (tests load the example
  languages) and [ADR 0020](0020-extension-pages.md) (the README renderer is the Markdown
  extension's file)

## Context

The extensions under the `flareite.` namespace (Git, Markdown, Images, and the languages HTML &
CSS, JavaScript, JSON, Python, Rust, TOML, TypeScript, and XML) are meant to be Flareite's official
extensions, released on their own schedules, so each gets a repository of its own. The editor
depended on them in three ways: its tests loaded their grammars and built and ran Git, Markdown,
and Images; and the extension host compiled the Markdown extension's renderer into itself, by
path, to show READMEs. A repository of its own would break all three.

## Decisions

### Nothing in the editor depends on an extension

The dependency runs one way: extensions build against the editor's API, and the editor builds
and tests without any of them. `extensions/` keeps the examples (Tidy, TODOs, Example Themes,
Example File Icons) and the test fixtures.

### Markdown rendering is the editor's, and an API (1.14)

The renderer moves into the host (`flareite-extension-host/src/markdown.rs`), where READMEs use
it, and extensions call it as **`workbench::render-markdown(text)`**, which returns preview
nodes. The Markdown extension no longer has a renderer or a Markdown parser of its own. A
Markdown preview and a README look the same, because one renderer draws both, and any extension
can show Markdown (a commit message, a hover) without bundling a parser.

- **No permission.** It reads nothing; the text is the extension's own.
- **At most 8 MiB in**, the most text a preview may hold, so whatever it returns fits in one.
  Larger is `too-large`. The output is cut to a preview's node limit with a note, as before.
- **In `workbench`**, beside the preview functions its result is for, rather than an interface
  of its own.

### Tests use fixtures, not the official extensions

- **Grammars.** `extensions/test-fixtures/languages/` holds language-only copies (ids `test.*`)
  of the Rust, TypeScript, JavaScript, JSON, TOML, Python, HTML & CSS, and Markdown grammars and
  queries, which the syntax crate, the app, and the host's tests load. They're not kept in step
  with the official extensions: tests need a fixed grammar, not the newest. Git stores the
  copies as the same objects the extensions had, so the repository doesn't grow.
- **Previews and viewers.** A fixture, `test.pages`, previews Markdown documents with
  `render-markdown` and shows files in a webview viewer that asks for their size. The host's
  preview and webview tests, which ran Markdown and Images, run it instead.
- **What the official extensions do** (the Git extension's staging, history, merges, blame, and
  so on; Markdown's checks; Images' picture details) is tested in their repositories, not here.

### The official extensions' repositories

Each is a standalone Cargo package (no workspace inheritance) that builds for
`wasm32-unknown-unknown`, with its manifest's entrypoint in its own `target/`. It depends on
`flareite-extension-api` by path, expecting this repository checked out beside it as `editor`.

## Consequences

- The host's integration tests no longer cover what only the Git extension used: tree views
  and their commands, prompts, provided documents and diffs, quick diff, the merge editor,
  timers, line annotations, and asking for credentials. Few of them have unit tests (prompts
  and quick diff have none); fixtures for them, like `test.pages`, would restore the coverage.
- A change to the API that breaks an official extension isn't caught here. Building the official
  extensions against a change is a step for whoever makes it.
- Extensions that call `render-markdown` need Flareite with API 1.14.
- The README renderer and previews can't drift apart, and the editor has one copy of the
  renderer instead of two.
