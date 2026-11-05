# ADR 0014: Languages only from extensions

- **Status:** Accepted
- **Date:** 2026-10-31

## Context

After [ADR 0013](0013-extension-languages.md), extensions could add languages, but Flareite still
compiled in seven (Rust, TypeScript, TSX, JavaScript, JSON, TOML, Python) and hard-coded three
language servers (rust-analyzer, typescript-language-server, pyright). Users should decide which
languages their editor has, and a language Flareite chose for them was the one kind they couldn't
remove, replace, or turn off.

## Decisions

### No built-in languages

`flareite-syntax` no longer depends on any grammar crate; its `Registry` holds only languages
from extensions. Without language extensions, every file is plain text. The seven former
built-in languages are now example extensions in `extensions/`, one per language (TypeScript
and TSX share one), so users take only the ones they want:

| Extension    | Languages                       | Server                       |
| ------------ | ------------------------------- | ---------------------------- |
| `rust`       | `rust`                          | `rust-analyzer`              |
| `typescript` | `typescript`, `typescriptreact` | `typescript-language-server` |
| `javascript` | `javascript`                    | `typescript-language-server` |
| `json`       | `json`                          | none                         |
| `toml`       | `toml`                          | none                         |
| `python`     | `python`                        | `pyright`                    |

- **Same highlighting.** Each extension's grammar is the prebuilt `.wasm` from the grammar's npm
  package, at the version the crate had, and its queries are the crate's, combined as Flareite
  combined them (TypeScript's queries add to JavaScript's, so the TypeScript extension carries
  its own copy of those). Before the native grammars were removed, a test compared both on real
  files from this repository: every line highlighted identically.
- **Cost.** WebAssembly grammars parse more slowly: a full parse of a 1,370-line Rust file took
  7.7 ms instead of 5.2 ms, and a keystroke's incremental reparse 164 µs instead of 144 µs
  (see [performance](../performance.md#languages-from-extensions)). That is well inside the
  3 ms inline budget; querying, the larger part of highlighting, is unchanged.
- **Not shipped.** The extensions are in the repository, not in the app. A user loads them as
  development extensions or installs them like any other extension.
- **Precedence.** With no built-in languages, the rule that an extension's language beats a
  built-in one is gone: when two languages claim a file equally, or share an id, the one
  registered first wins.

Alternatives considered:

- **Built-in extensions, shipped with the app and on by default.** A fresh install would still
  highlight code, and users could turn languages off. Rejected for now in favor of letting
  users choose from the start; the mechanism would be a resource folder of extensions loaded
  before installed ones.
- **One extension for all seven.** Simpler, but all or nothing.
- **Keeping the native grammars behind the extension interface.** Faster, but the grammars
  would still be in every binary, and not replaceable.

### No built-in language servers

`flareite-lsp` no longer has default servers. Servers come only from language extensions (behind
their `language-servers` permission) and from `[lsp.servers.*]` in the settings, which can still
change or turn off an extension's server by name, or add one.

- **Shared servers.** Two extensions may declare the same server, as the TypeScript and
  JavaScript extensions do. With the same name and command, they share one server serving all
  their languages; with a different command, the first declaration wins and the conflict is
  logged.
- **Permission.** Every server asks for the user's permission, since no extension is built in.
  If extensions are bundled later, granting their permission by default would restore the old
  experience for the servers that used to be built in.

### Benchmarks load languages explicitly

Benchmarks skip the user's extensions. `scripts/bench.mjs --language <folder>` (the app's
`--bench-language`) loads just that extension folder, so typing into a source file can still be
measured with highlighting.

## Consequences

- A fresh install highlights nothing until the user adds languages; the README, the extension
  guide, and the troubleshooting guide say how.
- The binary no longer contains grammar code; grammars are compiled when first used, as
  extension grammars already were. The example extensions add about 4.7 MB of `.wasm` files to
  the repository.
- Tests that need a language load the example extensions: `flareite-syntax` from the files, the
  app from the manifests.
- Adding or updating a language no longer touches Flareite's code or dependencies.
