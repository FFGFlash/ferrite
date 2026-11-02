import { useStore } from '../state/store'
import { dragStore } from './drag'
import './DragGhost.css'

/**
 * What's being dragged, beside the pointer. Files from other programs have
 * the system's own drag image instead.
 */
export function DragGhost() {
  const drag = useStore(dragStore)
  if (!drag || drag.payload.kind === 'files') {
    return null
  }
  return (
    <div
      className="drag-ghost"
      aria-hidden="true"
      data-copy={drag.copy || undefined}
      style={{ transform: `translate(${drag.x + 14}px, ${drag.y + 10}px)` }}
    >
      {drag.payload.label}
    </div>
  )
}
