/**
 * Drawing extension decorations. Each decorated range becomes rectangles in
 * the overlay layer (background, border, underline) and, for decoration
 * types that change the text itself (color, bold, italic), styled runs in
 * the line. Colors come from validated decoration types; they are checked
 * again here and used only as CSS colors.
 */
import type { CSSProperties } from 'react'
import type { DecorationView } from '../ipc/editor'
import type { DecorationColors, DecorationType } from '../ipc/extensions'
import type { ThemeKind } from '../themes/theme'
import type { TextOverlay } from './segments'

const HEX_COLOR = /^#(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i

const color = (value: string | null): string | undefined =>
  value && HEX_COLOR.test(value) ? value : undefined

function colorsFor(type: DecorationType, kind: ThemeKind): DecorationColors {
  return kind === 'light' ? type.light : type.dark
}

/** Style of an overlay rectangle for a decoration type. */
export function decorationRectStyle(type: DecorationType, kind: ThemeKind): CSSProperties {
  const colors = colorsFor(type, kind)
  const style: Record<string, string> = {}
  const background = color(colors.background)
  const border = color(colors.border)
  const underline = color(colors.underlineColor) ?? color(colors.color)
  if (background) {
    style['--decoration-background'] = background
  }
  if (border) {
    style['--decoration-border'] = border
  }
  if (type.underline && underline) {
    style['--decoration-underline'] = underline
  }
  return style as CSSProperties
}

/** Whether a decoration type styles the text itself. */
function textStyle(type: DecorationType, kind: ThemeKind): CSSProperties | null {
  const style: CSSProperties = {}
  const text = color(colorsFor(type, kind).color)
  if (text) {
    style.color = text
  }
  if (type.bold) {
    style.fontWeight = 'bold'
  }
  if (type.italic) {
    style.fontStyle = 'italic'
  }
  return Object.keys(style).length > 0 ? style : null
}

/**
 * Text overlays per line for decorations that style text, for lines
 * `startLine..startLine + lineLengths.length` (lengths in UTF-16 units).
 */
export function textOverlays(
  decorations: readonly DecorationView[],
  types: ReadonlyMap<string, DecorationType>,
  kind: ThemeKind,
  startLine: number,
  lineLengths: readonly number[],
): Map<number, TextOverlay[]> {
  const byLine = new Map<number, TextOverlay[]>()
  for (const decoration of decorations) {
    const type = types.get(decoration.key)
    const style = type && textStyle(type, kind)
    if (!style) {
      continue
    }
    const first = Math.max(decoration.start.line, startLine)
    const last = Math.min(decoration.end.line, startLine + lineLengths.length - 1)
    for (let line = first; line <= last; line++) {
      const length = lineLengths[line - startLine] ?? 0
      const from = line === decoration.start.line ? decoration.start.column : 0
      const to = line === decoration.end.line ? decoration.end.column : length
      if (to > from) {
        const overlays = byLine.get(line) ?? []
        overlays.push({ from, to, style })
        byLine.set(line, overlays)
      }
    }
  }
  return byLine
}
