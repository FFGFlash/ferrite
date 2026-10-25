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
}

/**
 * A vertical splitter that resizes the element before it. Supports pointer
 * dragging and keyboard adjustment (arrows, Shift+arrows, Home, End).
 */
export function Sash({ label, value, min, max, onChange }: SashProps) {
  const drag = useRef<{ startX: number; startValue: number } | null>(null)
  const change = (next: number) => onChange(Math.min(max, Math.max(min, Math.round(next))))

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return
    }
    event.preventDefault()
    event.currentTarget.setPointerCapture(event.pointerId)
    drag.current = { startX: event.clientX, startValue: value }
  }

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current) {
      change(drag.current.startValue + event.clientX - drag.current.startX)
    }
  }

  const endDrag = () => {
    drag.current = null
  }

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? KEY_STEP_LARGE : KEY_STEP
    switch (event.key) {
      case 'ArrowLeft':
        change(value - step)
        break
      case 'ArrowRight':
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
      className="sash"
      role="separator"
      aria-label={label}
      aria-orientation="vertical"
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
