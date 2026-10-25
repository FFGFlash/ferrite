/**
 * Minimap geometry, as in VS Code. Each document line is
 * `MINIMAP_LINE_HEIGHT` pixels tall. When the document (with the space for
 * scrolling past its last line) is taller than the minimap, the minimap
 * scrolls in proportion to the editor, so its top and bottom line up with the
 * editor's.
 */

/** Pixels per line in the minimap. */
export const MINIMAP_LINE_HEIGHT = 2
/** Pixels per column; the backend sends `MINIMAP_COLUMNS` columns. */
export const MINIMAP_CHAR_WIDTH = 1
export const MINIMAP_COLUMNS = 120
export const MINIMAP_WIDTH = MINIMAP_COLUMNS * MINIMAP_CHAR_WIDTH

/** The editor's scroll state. */
export interface EditorScroll {
  scrollTop: number
  scrollHeight: number
  clientHeight: number
}

export interface MinimapLayout {
  /** Minimap pixels scrolled past its top. */
  offset: number
  /** The slider, which covers the lines visible in the editor. */
  sliderTop: number
  sliderHeight: number
  /** The lines drawn: `firstLine..firstLine + lineCount`. */
  firstLine: number
  lineCount: number
}

/** The minimap's layout for a minimap `height` pixels tall. */
export function minimapLayout(
  scroll: EditorScroll,
  lineHeight: number,
  height: number,
): MinimapLayout {
  const ratio = MINIMAP_LINE_HEIGHT / lineHeight
  const total = scroll.scrollHeight * ratio
  const maxScroll = scroll.scrollHeight - scroll.clientHeight
  const scrollTop = Math.max(0, Math.min(scroll.scrollTop, maxScroll))
  const offset = total > height && maxScroll > 0 ? (scrollTop / maxScroll) * (total - height) : 0
  return {
    offset,
    sliderTop: scrollTop * ratio - offset,
    sliderHeight: scroll.clientHeight * ratio,
    firstLine: Math.floor(offset / MINIMAP_LINE_HEIGHT),
    lineCount: Math.ceil(height / MINIMAP_LINE_HEIGHT) + 1,
  }
}

/** The editor's scrollTop that centers the line at minimap point `y` (from its top). */
export function scrollTopFor(
  y: number,
  layout: MinimapLayout,
  scroll: EditorScroll,
  lineHeight: number,
): number {
  const line = Math.floor((y + layout.offset) / MINIMAP_LINE_HEIGHT)
  return line * lineHeight + lineHeight / 2 - scroll.clientHeight / 2
}

/** Editor pixels to scroll per pixel the slider is dragged. */
export function dragScale(scroll: EditorScroll, lineHeight: number, height: number): number {
  const ratio = MINIMAP_LINE_HEIGHT / lineHeight
  const maxScroll = scroll.scrollHeight - scroll.clientHeight
  // The slider's travel: the minimap's height less its own, once the
  // minimap scrolls; otherwise it moves with the lines.
  const travel = height - scroll.clientHeight * ratio
  if (scroll.scrollHeight * ratio <= height || maxScroll <= 0 || travel <= 0) {
    return 1 / ratio
  }
  return maxScroll / travel
}
