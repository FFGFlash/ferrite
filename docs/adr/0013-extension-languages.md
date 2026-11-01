# ADR 0013: Languages from extensions

- **Status:** Accepted; built-in languages and servers removed by
  [ADR 0014](0014-languages-only-from-extensions.md)
- **Date:** 2026-10-31

## Context

Ferrite highlights seven languages with Tree-sitter grammars compiled into the binary, and knows
three language servers. ADR 0005 left adding a language as a code change "until extensions can
contribute grammars". The design brief asks for extensions that "add language support" and for a
defined way to package, register, and associate grammars with files, without loading every
grammar up front.

Adding a language takes three things: which files it is for, a parser with queries that say
what to color, and optionally a language server. Real languages also nest: HTML embeds CSS and
JavaScript. The example that drove this work is HTML and CSS.

Extension code runs in a WebAssembly sandbox (ADR 0006) and must not be able to hurt the
editor. A grammar is code too, and so is a language server.

## Decisions

### Grammars are Tree-sitter grammars compiled to WebAssembly

An extension ships a grammar as the `.wasm` file `tree-sitter build --wasm` produces, and
Tree-sitter's own `wasm` feature loads it. The parser then runs inside Wasmtime, so a broken or
hostile grammar can't read memory outside its sandbox, and its memory is capped (128 MiB).

Alternatives considered:

- **Native grammar libraries (`.dll`/`.so`).** The standard way to load grammars at runtime and
  the fastest, but native code from an extension would run unsandboxed in the editor process.
  That undoes ADR 0006.
- **Declarative grammars (TextMate-style regular expressions).** No native code, but a second
  highlighting engine, no incremental parsing, and much weaker results than the trees the
  editor already uses.
- **Grammars bundled in Ferrite, chosen by extensions.** Safe and simple, but adding a language
  would still be a change to Ferrite, which is what this ADR is meant to end.

### One Wasmtime, at the version Tree-sitter uses

Tree-sitter 0.27's `wasm` feature depends on Wasmtime 48 (through its C API). Ferrite was on
Wasmtime 50.0.1 for its security fixes, and two Wasmtime versions would mean two copies of the
runtime and compiler in the binary and in build times. Wasmtime fixes security issues in its
supported older releases too: 48.0.8 was released the same day as 50.0.1 with the same fixes. So
the workspace pins Wasmtime 48.0.8, and moves on when Tree-sitter does.

Tree-sitter turns on Wasmtime's GC support, which Cargo then enables for the extension host's
Wasmtime as well. The extension engine turns WebAssembly GC off explicitly, keeping ADR 0006's
"no GC". Grammars get a separate engine, because the extension engine interrupts calls on a
schedule that Tree-sitter doesn't set up for its stores.

On Windows, Tree-sitter compiles its C code against Wasmtime's headers, which assume Wasmtime
is a DLL unless `LIBWASM_STATIC` is defined, so `.cargo/config.toml` defines it to avoid
linker warnings.

### A language registry instead of a fixed enum

`ferrite-syntax` replaces the `LanguageId` enum with a `Registry` of `Language`s: the bundled
ones, then extensions' `LanguageDefinition`s (id, name, file extensions and names, grammar and
query paths). A `Language` is a reference-counted registration, compared by identity. The app
rebuilds the registry in the background whenever extensions change, reusing it when no
definition or file modification time changed, so grammars aren't recompiled needlessly. A
document's syntax state is reset when its language or the registry changes, which is how an
edited query takes effect after restarting the extension.

- **Ids are global.** Language servers, settings, activation events, and other languages'
  injections refer to languages by id, so ids are not namespaced by extension. A contributed
  language whose id is taken is left out and logged.
- **Matching.** A whole file name beats an extension, a longer extension beats a shorter one,
  and an extension's language beats a built-in one that claims a file equally.
- **Lazy loading.** Grammar and query files are only checked for existence when the manifest
  loads; they're read and compiled the first time a document of the language is parsed.

### A parser per thread

A WebAssembly grammar must be instantiated in a store attached to the parser that uses it, and
creating a store instantiates Tree-sitter's runtime. Creating one per parse would cost every
keystroke. Instead each thread keeps one parser (with its store, added on first use of a
WebAssembly grammar), so a grammar is instantiated once per thread. A parse stopped by the time
budget resets the parser, which would otherwise resume it next time.

### Injections as layers of one syntax tree

`parse` now returns a `SyntaxTree`: the document's tree plus a layer per injected region. After
parsing, the language's injection query (`injections.scm`, with the usual
`@injection.content`, `injection.language` and `injection.combined` conventions) finds the
regions, and each is parsed with Tree-sitter's included ranges. Layers' own injections are
followed, up to 4 levels and 1,024 layers per document.

- **Incremental.** Edits are applied to every layer. On reparse, an old layer of the same
  language that overlaps a new region is passed as the old tree. That is safe even when the
  ranges moved, because Tree-sitter doesn't reuse nodes from where the included ranges
  differ. Tests check that incremental results match full parses through insertions inside,
  before, and around injected regions.
- **Budgeted.** Layers share the parse's stop condition, so the inline 3 ms budget and
  background cancellation (ADR 0005) cover the whole tree.
- **Painting.** Layers paint over the document's highlights, outermost first.

Alternatives considered:

- **Parsing injections at highlight time, for the viewport.** No state to maintain, but each
  view would reparse every visible embedded region from scratch, which breaks the keystroke
  budget for a large `<script>`.
- **One combined layer per language.** Fewer trees, but separate `<script>` elements would be
  parsed as one program, which is wrong for most injections.

### Language servers are data, behind a permission

An extension declares servers in `[[language_servers]]` (name, command, arguments, languages,
root markers), and they join the configured servers. Running a program is the most powerful
thing an extension can ask for, so it needs the new `language-servers` permission, which the
manifest must request exactly when it declares servers. The prompt shows each command line.

- **Ferrite runs them, not the extension.** The permission doesn't reach the WebAssembly API
  (`activate` never sees it), and extension code can't talk to the servers.
- **Commands** are programs on the `PATH`, or paths inside the extension's folder for servers
  an extension ships. Absolute paths are refused.
- **Settings stay in charge.** Built-in server names can't be taken over, and
  `[lsp.servers.<name>]` can change or turn off an extension's server. Settings entries for
  unknown servers no longer need to be complete, since they may refer to an extension's server.
- **Changes restart the servers.** When the set of allowed servers changes, all servers are
  restarted with the new configuration, as when the `[lsp]` settings change.

### Language-only extensions need no code

Like theme and icon-theme extensions, an extension that only contributes languages and language
servers has no entrypoint. Such an extension still waits for permission (shown as "Needs
permission") if it declares servers; its languages work either way.

## Consequences

- Adding a language is a folder: `extension.toml`, a `.wasm` grammar, and queries.
  `extensions/html-css` adds HTML (with CSS and JavaScript inside it) and CSS, plus their
  language servers from `vscode-langservers-extracted`. Its grammars are the prebuilt files from
  the `tree-sitter-html` 0.23.2 and `tree-sitter-css` 0.25.0 npm packages (MIT).
- Grammars are trusted not to loop. Tree-sitter's progress callback stops a parse between
  steps, but Ferrite can't interrupt a grammar's external scanner inside a call, because
  Tree-sitter's stores have no deadline to set. A scanner that never returns blocks its thread,
  and an inline parse holds the editor lock. The guide says to use grammars from trusted
  sources.
- Bundled grammars still don't use injections; their queries are unchanged.
- Wasmtime upgrades now wait for Tree-sitter's.
