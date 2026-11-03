# Images

Shows image files as pictures instead of as text, with what they are:

> PNG · 1920 × 1080 pixels · 8-bit RGBA · 245.3 KiB

PNG (and animated PNG), JPEG, GIF, WebP, BMP, ICO, and AVIF files open in the **Image Viewer**.
SVG files open as text, since they're text people edit; to see one as a picture, run
**View: Reopen Editor With…** and choose **Image Viewer**. The same command opens any image as
text.

## Using the viewer

The picture is centered in the tab. The toolbar above it has:

- **Backgrounds** to show it on, so its transparent parts show: a checkerboard in the theme's
  colors, a light or dark checkerboard, none (the editor's background), white, or black. The
  choice applies to every image tab, and is kept for next time.
- **Fit** (the default): the whole picture in the tab. SVG pictures grow to fill it; bitmaps only
  shrink, so small ones stay sharp.
- **Actual size** (1:1), and **zoom out** and **zoom in**, from 10% to 3200%. Bitmaps zoomed past
  100% show their pixels as squares.

Ctrl+mouse wheel zooms about the pointer. With the picture focused, `+` and `-` zoom, `0` shows
it at actual size, and `F` fits it (with or without Ctrl). Each tab keeps its own zoom, also
when it's hidden and shown again.

The viewer follows the file: when another program changes it, the picture and its details
update.

## How it works

This is the example of a webview viewer (see "Webviews" in Ferrite's `docs/extensions.md`). The
manifest declares two viewers, with the file names they're for, that both show
`media/viewer.html`. The page:

- loads the picture from `ferrite.fileUrl`, and again when `ferrite.onFileChanged` says it
  changed;
- draws its toolbar with Ferrite's kit (`__ferrite/kit.css`) and named icons, so it looks like
  the workbench in any theme;
- keeps the background with `ferrite.storage`, and follows other tabs' changes with
  `ferrite.onStorageChanged`; keeps each tab's zoom with `ferrite.setState`;
- asks for the picture's details with `ferrite.postMessage`.

That message runs the viewers' `on_message` command, in the extension's code (`src/lib.rs`). It
reads the start of the file (`viewers::read-viewed-file`), works out the format and size from its
header (`src/info.rs`, which has its own tests), and answers with the details line and the size
in pixels (`webviews::post-message`). The page uses that size as an SVG picture's actual size,
which browsers don't know when the file has only a `viewBox`.

It needs no permissions: a viewer can read only the files open in it, and its page can't reach
the network.
