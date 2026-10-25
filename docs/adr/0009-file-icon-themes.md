# ADR 0009: File icon themes

- **Status:** Accepted
- **Date:** 2026-10-25

## Context

The explorer shows an icon for each file and folder, chosen from a built-in table (Lucide icons
by extension and well-known name). Users want extensions to replace those icons, as icon theme
extensions such as file-icons do in VS Code.

This is a new kind of workbench contribution (ADR 0007), and it is the first that carries images
rather than text and colors, so how icons reach the page matters for safety.

## Decisions

### One active icon theme, modeled on VS Code's

An extension declares `[[icon_themes]]` (`id`, `label`, `path`), and the user picks one with
`workbench.icon_theme` or **Preferences: File Icon Theme: …**. `default` is the built-in table.

- **The theme file** (TOML) names icons (`[icons]`: id to file) and associates them with file
  extensions, file names, and folder names (closed and open). It can also set default `file`,
  `folder`, and `folder_expanded` icons, and a `[light]` section with the same tables that is
  looked up first while a light color theme is active.
- **Matching.** Names are matched case-insensitively. A file name beats an extension, and the
  longest extension wins (`d.ts` beats `ts`).
- **Fallback.** Anything the theme doesn't match uses its defaults, and without those the
  built-in icons. A theme can therefore restyle a few file types or take over completely.

Alternatives considered:

- **Every enabled extension adds icons, merged in some order.** This avoids a setting, but it
  needs precedence rules between extensions and mixes icon styles. Icon sets are designed as a
  whole, which is why VS Code also has one active theme.
- **Extensions pick from the built-in Lucide icons and set a color.** This is simpler and needs no
  images, but it can't show language logos, which are what icon theme extensions are mostly
  for.

### Icons are images, never markup

Icons are SVG or PNG files. The backend reads and checks them when the manifest loads:

- **Paths.** They are relative to the theme file and stay inside it.
- **Contents.** The extension must match the contents: a PNG signature, or UTF-8 text containing
  `<svg`.
- **Limits.** Each icon is at most 64 KiB, with 2,000 icons and 8 MiB per theme.

The frontend receives icons as base64 `data:` URLs and shows them with `<img>`. An image
document can't run script or load other resources, so an SVG can't do anything inside the
workbench, and the CSP (`img-src 'self' data:`) needs no change. Inlining SVG markup would have
let icons follow `currentColor`, but it would mean sanitizing extension markup. The `[light]`
section covers the same need.

### Loaded with the manifest, sent on request

A theme is validated when its extension loads, so mistakes show in the Extensions view like a
broken color theme. The icons are kept behind an `Arc` and left out of the extension list,
which the Extensions view polls every few seconds. The frontend fetches the active theme
(`extensions_icon_theme`) when the setting or the extension list changes, and keeps its
associations in `Map`s so names like `constructor` can't reach object prototypes.

## Consequences

- **Replacing icons needs no code.** An extension that only contributes icon themes (or color
  themes) needs no entrypoint and never runs. `extensions/icons` is an example.
- **Memory.** Every installed icon theme's icons stay in memory, up to 8 MiB each and 5 per
  extension. Loading only the active theme's icons would save that, but problems would then
  appear only when a theme is chosen.
- **Icons don't follow theme colors.** Images can't use theme tokens, so a theme that wants
  different icons on light backgrounds provides them in `[light]`.
- **Explorer and Go to File only.** Tabs and search results still show no file icons. They can
  use the same resolver (`components/fileIcons.ts`) and `FileIcon` component.
