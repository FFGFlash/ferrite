# ADR 0025: Webviews

- **Status:** Accepted
- **Date:** 2026-11-02

## Context

Extensions show things in tabs through preview content ([ADR 0016](0016-previews.md)): typed
nodes the workbench draws, so an extension never supplies markup. Viewers
([ADR 0023](0023-encodings-and-viewers.md)) used the same content, and the workbench styled a
viewer's picture itself (centered, on a checkerboard).

That ran out with the first request beyond it: an image viewer like VS Code's SVG preview, with
a choice of background (to see transparency), the picture centered, and its size changeable
(fit, 1:1, zoom). Content nodes have no controls or events, and adding the controls to the
workbench would have made it know about one extension. The requirement was the opposite: give
extension authors a way to build such tabs themselves, and make it pleasant to do.

ADR 0007 and 0016 ruled out webviews as the default for memory and security surface, and left a
general UI protocol for later. A UI protocol (element trees and events) would need its own
widgets, layout, and event model for every kind of tab, and still couldn't draw a zoomable
picture without a canvas-like element. Webviews let authors use the web platform they already
know, if the sandbox is tight enough.

## Decisions

### Viewers and previews can be webviews, in API 1.5

A viewer or preview whose manifest entry has `webview` shows a page instead of content:
`webview = "media/viewer.html"` (a page in the extension's folder), or `webview = true` for HTML
the extension sets with `webviews::set-webview-html`. Content-based viewers and previews are
unchanged, and Markdown keeps its typed preview.

- **A new `webviews` interface**: `set-webview-html`, `post-message`, and `message`. A tab is
  named as `viewer(viewer, path)` or `preview(preview, key)`.
- **Messages from the page run a command** (the entry's `on_message`), during which
  `webviews::message()` returns it, as `command-argument` does for menus. Adding an export would
  break older components ([ADR 0016](0016-previews.md)), and commands already start the
  extension when needed.
- **A webview viewer's `command` is optional.** The page loads the file itself, through an
  address that serves only that file.
- **Compatibility.** Only interfaces, functions, and types are added (`webviews`, `storage`,
  new records and a variant in `types`), so components built against 1.0 to 1.4 still link; the
  host's tests still load the 1.0, 1.1, and 1.2 builds.

### The sandbox

- **A frame with `sandbox="allow-scripts"` only.** The page's origin is opaque: it can't reach
  the workbench's document, Ferrite's IPC, cookies, or storage, and it can't navigate the window,
  open popups, or submit forms.
- **A protocol of Ferrite's own**, `ferrite-webview:` (`http://ferrite-webview.localhost` on
  Windows), serves pages at `<origin>/<token>/<path>`. The token is 128 random bits made for one
  open tab; a request names a tab only by its token. Files are the extension's, confined to its
  folder after following symlinks, plus the viewed file at `__ferrite/file` and Ferrite's own
  files under `__ferrite/`.
- **A content security policy on every response**, which a page can tighten but not loosen:
  only its own address (with its token), `data:`, and `blob:`; inline scripts and styles
  allowed; no forms, no base URL, no other frames.
- **The `network` permission** adds `https:` for scripts, styles, images, media, fonts, and
  `fetch`, and `wss:` for sockets, once the user grants it. Pages still can't navigate.
- **Messages go through the workbench.** It acts only on messages from its own frame's window, in
  the bridge's shape, and checks each kind's fields. The host checks again: messages are JSON, at
  most 1 MiB.

### The bridge

Ferrite injects a script before the page's own (`ferrite-extension-host/src/webview.js`). It
gives the page a `ferrite` object: the viewed file's address and changes, messages both ways
(those sent before the page listens wait for it), tab state, storage, the theme, and tab
actions. Messages the extension sends before the page has loaded wait in the workbench
(`state/webviews.ts`).

### State and storage

- **Tab state** (`getState`, `setState`, at most 64 KiB) comes back when the page loads again.
  `getState` is synchronous: the host puts the state in the bridge as it serves it. A viewer's
  state is also saved for the next session (`extension-storage/webview-state.json`), since its
  tab is saved with the layout, and forgotten when the tab closes.
- **Extension storage**: small JSON values in a `global` scope and one per folder (`workspace`),
  at most 1 MiB each, shared by the extension's code (the `storage` interface) and its pages
  (`ferrite.storage`, `ferrite.workspaceStorage`). Pages hear of changes made elsewhere
  (`onStorageChanged`). It needs no permission: it holds only what the extension put there. The
  Extensions view shows how much an extension stores, with **Clear Stored Data**.

### Lifetime

The dock draws only each group's active tab, so a hidden page is unloaded and loads again (with
its state) when shown. An extension can set `retain_when_hidden = true` to keep its page loaded
instead: the group keeps the frame, hidden, once shown. That costs memory, so it's the
extension's choice, as in VS Code.

### Looking like Ferrite

- **The theme** reaches every page as CSS variables, `--ferrite-<token>` for each color token
  and a few others (fonts, sizes, radius, color scheme). The workbench sends them to the host,
  which puts them in pages as it serves them (no flash of other colors), and to loaded pages when
  the theme changes.
- **Defaults** for body text, links, focus rings, selection, and scroll bars come with them, in
  `:where()`, so they have no specificity and any rule of the page's wins.
- **The kit** (`__ferrite/kit.css`, found from any folder) is opt-in: toolbars, buttons, inputs,
  labels, and named icons in the text color.
- **Named icons** are one set for tab actions and the kit, `src/components/actionIcons.json`,
  generated from Lucide by `scripts/action-icons.mjs`.

### Tab actions

`[[viewers.actions]]` and `[[previews.actions]]` declare buttons beside the tab: an id, a title,
and a named icon or an SVG or PNG in the extension's folder (loaded with the manifest, at most
32 KiB). A click runs the action's command with the tab as its argument, or, without one, goes
to the page, which can set each button's pressed, disabled, hidden, and title. Any viewer or
preview can have actions with commands; actions without one need a webview.

### Keyboard

A focused frame gets key presses, not the workbench, so Ferrite's shortcuts would stop working
in a page. The workbench sends each page the key combinations bound outside the editor and
terminal (`webviewKeystrokes`), and the bridge passes on a press that matches one, and the next
key of a chord, unless the page handled it (`preventDefault` in a listener on an element or the
document). It blocks the browser's default for those keys. The workbench gets them as key presses
on the frame. Other keys (typing, Ctrl+C, Ctrl+Left in a field) stay in the page.

Alternatives considered:

- **Image viewing in the workbench.** Least code, but it's what the requirement ruled out: the
  workbench would know about one extension, and no other extension could build a similar tab.
- **Interactive content nodes** (toolbars, buttons, picture options, with state in the
  extension). Keeps the no-markup rule, but each new kind of tab needs new node types, the
  extension holds UI state across messages, and a zoomable picture still needs the workbench to
  know about pictures.
- **A declarative picture viewer element.** Simple for authors, but only for pictures.
- **HTML set only from code** (VS Code's `webview.html`). Kept as an option (`webview = true`),
  but pages in the extension's folder are easier to write and need no code at all.
- **Workbench first for shortcuts.** Simpler, but pages couldn't use keys Ferrite binds, such as
  Ctrl+= in the Images viewer.

## Consequences

- **The no-markup rule now has an exception.** Webview pages are markup and script from the
  extension; they're contained by the sandbox and the policy instead of by construction.
- **Tauri's IPC script is injected into frames too.** On Windows the webview runs the editor's
  initialization scripts in every frame, so pages have `__TAURI_INTERNALS__`. It can't reach the
  backend: the policy blocks the `ipc:` protocol, and WebView2 doesn't deliver frames' web
  messages (checked in the real app: a page's `invoke` never ran, while the same call from the
  window did). The bridge removes `chrome.webview` as well. A future WebView2 or Tauri that
  delivers frame messages would need a check of the IPC request's origin.
- **Memory.** Each loaded page is a document in the webview; pages kept with
  `retain_when_hidden` stay so while hidden. Moving a kept tab to another group loads it again
  (an `iframe` can't be moved without reloading).
- **Page first only goes so far.** A page's `keydown` listener on `window` runs after the
  bridge's, too late to keep a key from the workbench; the docs say to listen on the document.
- **Pages are served from memory**: the protocol reads a whole file per request (up to 256 MiB),
  like pictures in previews.
- **The Images example is a webview.** Its page draws the picture with backgrounds, fit, 1:1,
  zoom, and Ctrl+wheel; its code reads the header for the details line and an SVG file's size.
  The workbench's styling of pictures in content viewers stays, for other extensions.
