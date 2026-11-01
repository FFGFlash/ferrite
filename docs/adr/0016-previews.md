# ADR 0016: Previews

- **Status:** Accepted
- **Date:** 2026-11-01

## Context

A Markdown extension should highlight Markdown files and show the page rendered beside the
editor, updating as you type. Highlighting is a language (ADR 0013) and needs no new mechanism.
Rendering does: extensions can fill in status items, view trees, and decorations (ADR 0007), none
of which can show formatted text. ADR 0007 left "a richer protocol" for a later contribution type
and ruled out sandboxed webviews by default. The rule that extensions never supply markup or CSS
still holds.

## Decisions

### A contribution type for formatted text, in API 1.2

Manifests declare `[[previews]]` (an id and a tab icon). Three functions join the `workbench`
interface in `ferrite:extension@1.2.0`: `show-preview`, `set-preview-content`, and
`close-preview`. One preview type can have many open previews, keyed by a string the extension
picks (Markdown uses the document's id).

- **Compatibility.** The change only adds types and functions, so components built against 1.0
  and 1.1 still link. The host's tests load a TODOs build made against 1.1, as they do a Tidy
  build made against 1.0.
- **Knowing the tab closed.** Adding an export would break older components, so
  `set-preview-content` returns `false` instead when the preview isn't open, and the extension
  stops updating it.

### Content is typed nodes, drawn by the workbench

The content is a flat list of nodes, each naming the index of the node it's inside (WIT has no
recursive types): paragraphs, headings, quotes, lists and tasks, code blocks, rules, tables,
text, emphasis, strong, strikethrough, inline code, links, pictures, and line breaks. That is
CommonMark with GitHub's tables, task lists, and strikethrough, without raw HTML.

- **Checked in the host** (`preview.rs`): what may contain what (so the frontend never builds
  invalid DOM), parents before children, depth (64), node count (100,000), and text (8 MiB).
- **Links and pictures are resolved in the host,** against the previewed document's folder:
  web addresses (`http:`, `https:`, `mailto:`), headings in the preview, and relative files. Other
  schemes and absolute paths become text. The frontend acts only on the resolved kinds, and the
  backend opens web addresses only after checking the scheme again.
- **Pictures** load through the backend as `data:` URLs, and only files the preview's content
  names can be read. Web pictures aren't loaded, so a preview makes no network requests (the CSP
  forbids them anyway); their description is shown.
- **Code blocks** are highlighted with the editor's grammars when the frontend fetches content,
  within 200 ms. Blocks left plain because time ran out (compiling a grammar) are flagged, and
  the frontend fetches once more.

Alternatives considered:

- **Sandboxed HTML** (an iframe with a strict CSP, or sanitized HTML). It's what other editors
  do, and would show any HTML in Markdown. But the extension would supply markup, a sanitizer
  becomes security-critical code, and theming would be the extension's job. ADR 0007 ruled it out
  as the default.
- **A Markdown renderer in the workbench,** given the document's text. The extension would be
  trivial, but Markdown would be built in, which ADR 0014 moved away from, and no other format
  could use it.
- **A general UI protocol** (element trees with events). Much larger to specify and secure than
  previews need. Previews can grow toward it, as their own types.

### Previews are dock items

An open preview is a tab, `preview:<id>`, that can be docked anywhere like documents and
terminals (ADR 0012). `state/previews.ts` follows the backend's list, and the dock follows it.

- **Placement.** A preview opens in its document's group, or beside it (in the group to the
  right, or a new one) when the extension asks. Opened beside, focus stays in the editor, so
  typing carries on and updates it.
- **Lifetime.** A preview closes with its document or its extension, and when its tab closes
  (`extension_preview_closed`). Previews aren't saved with the layout; extensions reopen them.
- **Updates.** The host drops unchanged content, and the tab runs one fetch at a time, so an
  extension can render on every change.

### The Markdown example

`extensions/markdown` has two languages, `markdown` and `markdown_inline`, from
tree-sitter-markdown 0.5.3's prebuilt grammars. The block grammar injects the inline one into
paragraphs, headings, and cells, and code fences into their language. Its queries use the
capture names Ferrite colors, so emphasis gets a color rather than a style. Its code renders
with pulldown-cmark on **Open Preview** (`Ctrl+Shift+V`) and **Open Preview to the Side**
(`Ctrl+K V`), and again on every change to the document.

Injections now find a language by file extension when the name isn't a language id
(`Registry::for_name`), so code fences written as `ts` or `rs` are highlighted.

## Consequences

- **What Markdown loses.** Raw HTML is reduced to its text (`<br>` becomes a line break, and
  comments disappear). Footnotes, math, root-relative links (`/docs/x.md`), and web pictures
  aren't shown.
- **Emphasis has no font style.** Syntax colors can't be bold or italic, so in the editor
  emphasis and strong text get the type and constant colors.
- **First highlights of a fenced file are slower,** because opening one compiles the grammars of
  its fences too.
- **New dependency.** `open` (with `is-wsl` and `is-docker`) opens links with the system. Tauri's
  opener plugin would have added about 35 crates for features the backend doesn't use.
