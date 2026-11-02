# Images

Shows image files as pictures instead of as text, with what they are:

> PNG · 1920 × 1080 pixels · 8-bit RGBA · 245.3 KiB

PNG (and animated PNG), JPEG, GIF, WebP, BMP, ICO, and AVIF files open in the **Image Viewer**.
SVG files open as text, since they're text people edit; to see one as a picture, run
**View: Reopen Editor With…** and choose **Image Viewer**. The same command opens any image as
text.

The viewer follows the file: when another program changes it, the picture and its details
update.

## How it works

This is the example of a viewer (see "Viewers" in Ferrite's `docs/extensions.md`). The manifest
declares two
viewers, with the file names they're for and the command that fills them in. The command reads
the start of the file (`viewers::read-viewed-file`), works out the format and size from its
header (`src/info.rs`, which has its own tests), and shows the picture and that line
(`viewers::set-viewer-content`). The workbench draws the picture from the file itself.

It needs no permissions: a viewer can read only the files open in it.
