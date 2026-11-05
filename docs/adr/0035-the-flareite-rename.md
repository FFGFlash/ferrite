# ADR 0035: The Flareite rename

- **Status:** Accepted
- **Date:** 2026-11-05

## Context

The project was called Ferrite, which is already the name of a code editor written in Rust. It is
now Flareite: the same orange feel and the same "-ite" ending, and a coined word, so it's easier to
search for. Nothing has been released, so there are no users or published extensions to keep
working.

## Decisions

- **Everything is renamed at once.** Crates (`flareite-*`), the app and its binary, the app
  identifier (`dev.flareite.editor`), environment variables (`FLAREITE_*`), the webview scheme
  (`flareite-webview:`), the reserved `__flareite/` paths, the `flareite` object pages use, the
  webview kit's `flareite-*` classes and variables, the official extensions' namespace
  (`flareite.*`), and the docs, including earlier ADRs (they were rewritten, not left saying
  Ferrite).
- **The WIT package is `flareite:extension`.** Components import interfaces by package name, so
  components built against `ferrite:extension` no longer link. The version (1.14.0) and
  `api_version = "1"` are unchanged, since nothing else about the interface changed; the minor
  versions in the docs still say when each function arrived.
- **No compatibility with old builds.** The host's tests loaded Tidy built against 1.0 and 1.2 and
  TODOs built against 1.1, to show older minor versions still link. Those builds import the old
  package, so they and their tests are gone. The next minor version can add such a fixture again,
  built against 1.14.
- **No migration of user data.** The app identifier decides where settings, session state,
  permissions, extension storage, and installed extensions live, so Flareite starts with none of
  Ferrite's. They can be moved by hand. Permissions and storage kept under `ferrite.*` extension
  ids would not apply to the renamed extensions anyway.

## Consequences

- Every extension must be rebuilt against the renamed API, and its manifest's id and commands
  renamed if they were under `ferrite.`.
- Git history, the GitHub repository's name, and folder names on disk still say Ferrite.
