import { useRef, type KeyboardEvent, type PointerEvent } from 'react'
import './Sash.css'

const KEY_STEP = 16
const KEY_STEP_LARGE = 64

interface SashProps {
  label: string
  value: number
  min: number
  max: number
  onChange: (value: number) => void
  /**
   * `vertical` (the default) sits on the right edge of the element it
   * resizes; `horizontal` sits on the top edge, so dragging up grows it.
   */
  orientation?: 'vertical' | 'horizontal'
}

/**
 * A splitter that resizes the element it's in. Supports pointer dragging and
 * keyboard adjustment (arrows, Shift+arrows, Home, End).
 */
export function Sash({ label, value, min, max, onChange, orientation = 'vertical' }: SashProps) {
  const horizontal = orientation === 'horizontal'
  const drag = useRef<{ start: number; startValue: number } | null>(null)
  const change = (next: number) => onChange(Math.min(max, Math.max(min, Math.round(next))))
  const position = (event: PointerEvent<HTMLDivElement>) =>
    horizontal ? -event.clientY : event.clientX

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return
    }
    event.preventDefault()
    event.currentTarget.setPointerCapture(event.pointerId)
    drag.current = { start: position(event), startValue: value }
  }

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current) {
      change(drag.current.startValue + position(event) - drag.current.start)
    }
  }

  const endDrag = () => {
    drag.current = null
  }

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? KEY_STEP_LARGE : KEY_STEP
    const [shrink, grow] = horizontal ? ['ArrowDown', 'ArrowUp'] : ['ArrowLeft', 'ArrowRight']
    switch (event.key) {
      case shrink:
        change(value - step)
        break
      case grow:
        change(value + step)
        break
      case 'Home':
        change(min)
        break
      case 'End':
        change(max)
        break
      default:
        return
    }
    event.preventDefault()
  }

  return (
    <div
      className={horizontal ? 'sash sash--horizontal' : 'sash'}
      role="separator"
      aria-label={label}
      aria-orientation={orientation}
      aria-valuenow={value}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onKeyDown={onKeyDown}
    />
  )
}
