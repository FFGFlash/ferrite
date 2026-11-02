import type { ReactNode } from 'react'
import { segments, type TextOverlay } from './segments'

/** `text` split into plain, highlighted, decorated, and linked runs (see `measure.ts`). */
export function highlighted(
  text: string,
  spans: readonly number[],
  overlays: readonly TextOverlay[],
): ReactNode[] {
  return segments(text, spans, overlays).map(({ start, end, className, style }) =>
    className || style ? (
      <span key={start} className={className || undefined} style={style}>
        {text.slice(start, end)}
      </span>
    ) : (
      text.slice(start, end)
    ),
  )
}
