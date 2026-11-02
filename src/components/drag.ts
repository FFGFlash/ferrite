/**
 * Dragging in the workbench: tabs, activity bar buttons, and explorer rows,
 * moved with the pointer, and files dragged in from other programs, which
 * the backend reports as they move (`fileDrag` events).
 *
 * HTML5 drag and drop isn't used. Files dragged in from other programs come
 * through Tauri's drop handler, which on Windows replaces WebView2's, and
 * HTML5 drags stop working with it. So a drag here is a pointer held down
 * and moved. Drop targets register on elements (`useDropTarget`), and the
 * element under the pointer and its ancestors are asked, innermost first,
 * whether they take the drag, as `dragover` bubbles.
 */
import { useCallback, useRef, type PointerEvent as ReactPointerEvent } from 'react'
import { isMac } from '../editor/keymap'
import { createStore } from '../state/store'

export type DragPayload =
  /** A tab or activity bar button. */
  | { kind: 'dock'; item: string; label: string }
  /** Files and folders dragged in the explorer. */
  | { kind: 'entries'; paths: readonly string[]; label: string }
  /**
   * Files and folders dragged in from another program. `dropId` names the
   * drop to the backend, once dropped (for copying them).
   */
  | { kind: 'files'; files: readonly string[]; folders: readonly string[]; dropId: number | null }

export interface Drag {
  payload: DragPayload
  /** The pointer, in CSS pixels from the window's top left. */
  x: number
  y: number
  /** Whether the copy key is held: Ctrl, or Option on macOS. Never for files from other programs. */
  copy: boolean
}

export interface DropHandlers {
  /** Whether this element takes the drag where the pointer is. Called as it moves. */
  over(drag: Drag, element: HTMLElement): boolean
  drop(drag: Drag, element: HTMLElement): void
  /** The drag left this element: it ended, or the pointer moved to another target. */
  leave?(): void
}

/** The drag under way, if any. */
export const dragStore = createStore<Drag | null>(null)

/** Pointer movement before a press becomes a drag, in pixels. */
const DRAG_THRESHOLD = 4
/** Dragging this close to the edge of a scrolling area scrolls it, in pixels. */
const SCROLL_EDGE = 24
/** Pixels per frame at the very edge. */
const SCROLL_SPEED = 12

const targets = new WeakMap<Element, { current: DropHandlers }>()
/** The target that took the drag where the pointer last was. */
let active: { element: HTMLElement; handlers: { current: DropHandlers } } | null = null
/** What a drop nothing takes does. */
let fallback: ((drag: Drag) => void) | null = null

/** Makes an element a drop target; pass the result as its `ref`. */
export function useDropTarget<T extends HTMLElement>(
  handlers: DropHandlers,
): (element: T | null) => void {
  const ref = useRef(handlers)
  ref.current = handlers
  return useCallback((element: T | null) => {
    if (element) {
      targets.set(element, ref)
    }
  }, [])
}

/** Handles drops no target takes, such as files dropped on the status bar. */
export function setDropFallback(handler: (drag: Drag) => void): void {
  fallback = handler
}

/** Asks the targets under the pointer, innermost first, until one takes the drag. */
function hitTest(drag: Drag): void {
  let taken: typeof active = null
  for (
    let element = document.elementFromPoint(drag.x, drag.y);
    element;
    element = element.parentElement
  ) {
    const handlers = targets.get(element)
    if (handlers?.current.over(drag, element as HTMLElement)) {
      taken = { element: element as HTMLElement, handlers }
      break
    }
  }
  if (active && active.element !== taken?.element) {
    // Leaving may clear what the new target just showed, so it's asked again.
    active.handlers.current.leave?.()
    taken?.handlers.current.over(drag, taken.element)
  }
  active = taken
}

function update(drag: Drag): void {
  dragStore.set(drag)
  hitTest(drag)
}

/** Ends the drag: drops it (on the target under the pointer, else the fallback), or cancels it. */
function finish(drag: Drag | null): void {
  const target = active
  active = null
  delete document.body.dataset.dragging
  if (drag) {
    if (target) {
      target.handlers.current.drop(drag, target.element)
    } else {
      fallback?.(drag)
    }
  }
  target?.handlers.current.leave?.()
  dragStore.set(null)
}

const copyHeld = (event: { ctrlKey: boolean; altKey: boolean }) =>
  isMac() ? event.altKey : event.ctrlKey

/**
 * Starts watching a press for a drag (call from `onPointerDown`). Once the
 * pointer moves a few pixels, `payload` says what's dragged (or `null` for
 * nothing). Escape cancels. The click that ends a drag doesn't reach the
 * element.
 */
export function startDrag(
  event: ReactPointerEvent<HTMLElement>,
  payload: () => DragPayload | null,
): void {
  if (event.button !== 0 || event.pointerType === 'touch' || dragStore.get()) {
    return
  }
  const source = event.currentTarget
  const { pointerId } = event
  const start = { x: event.clientX, y: event.clientY }
  let drag: Drag | null = null

  const move = (e: PointerEvent) => {
    if (e.pointerId !== pointerId) {
      return
    }
    if (!drag) {
      if (Math.hypot(e.clientX - start.x, e.clientY - start.y) < DRAG_THRESHOLD) {
        return
      }
      const what = payload()
      if (!what) {
        stop()
        return
      }
      drag = { payload: what, x: e.clientX, y: e.clientY, copy: copyHeld(e) }
      try {
        // Keeps the pointer's events coming over iframes and outside the window.
        source.setPointerCapture?.(pointerId)
      } catch {
        // The pointer is already gone.
      }
      window.getSelection()?.removeAllRanges()
      document.body.dataset.dragging = ''
      autoScroll(() => drag)
    }
    e.preventDefault()
    drag = { ...drag, x: e.clientX, y: e.clientY, copy: copyHeld(e) }
    update(drag)
  }
  const up = (e: PointerEvent) => {
    if (e.pointerId !== pointerId) {
      return
    }
    const dropped = drag
    stop()
    if (dropped) {
      swallowClick()
      finish(dropped)
    }
  }
  const cancel = () => {
    const cancelled = drag
    stop()
    if (cancelled) {
      finish(null)
    }
  }
  const key = (e: KeyboardEvent) => {
    if (!drag) {
      return
    }
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopImmediatePropagation()
      cancel()
    } else if (copyHeld(e) !== drag.copy) {
      drag = { ...drag, copy: copyHeld(e) }
      update(drag)
    }
  }
  // The window losing focus ends the drag; elements blur as the press moves
  // focus, and those blurs reach this capturing listener too.
  const blur = (e: FocusEvent) => {
    if (e.target === window) {
      cancel()
    }
  }
  const listeners = [
    ['pointermove', move],
    ['pointerup', up],
    ['pointercancel', cancel],
    ['keydown', key],
    ['keyup', key],
    ['blur', blur],
  ] as const
  const stop = () => {
    drag = null
    for (const [type, listener] of listeners) {
      window.removeEventListener(type, listener as EventListener, true)
    }
  }
  for (const [type, listener] of listeners) {
    window.addEventListener(type, listener as EventListener, true)
  }
}

/** Keeps the click that ends a drag from activating what's under it. */
function swallowClick(): void {
  const swallow = (event: MouseEvent) => {
    event.preventDefault()
    event.stopPropagation()
  }
  window.addEventListener('click', swallow, { capture: true, once: true })
  setTimeout(() => window.removeEventListener('click', swallow, true), 0)
}

/** The nearest ancestor that scrolls, and the directions it can. */
function scroller(
  element: Element | null,
): { element: HTMLElement; x: boolean; y: boolean } | null {
  for (let node = element; node instanceof HTMLElement; node = node.parentElement) {
    const style = getComputedStyle(node)
    const y = /auto|scroll/.test(style.overflowY) && node.scrollHeight > node.clientHeight
    const x = /auto|scroll/.test(style.overflowX) && node.scrollWidth > node.clientWidth
    if (x || y) {
      return { element: node, x, y }
    }
  }
  return null
}

/** While a drag lasts, scrolls the area under the pointer when it's near its edge. */
function autoScroll(current: () => Drag | null): void {
  const step = (distance: number) =>
    distance < SCROLL_EDGE ? Math.ceil(SCROLL_SPEED * (1 - Math.max(0, distance) / SCROLL_EDGE)) : 0
  const tick = () => {
    const drag = current()
    if (!drag) {
      return
    }
    const area = scroller(document.elementFromPoint(drag.x, drag.y))
    if (area) {
      const r = area.element.getBoundingClientRect()
      const dy = area.y ? step(r.bottom - drag.y) - step(drag.y - r.top) : 0
      const dx = area.x ? step(r.right - drag.x) - step(drag.x - r.left) : 0
      if (dx !== 0 || dy !== 0) {
        area.element.scrollBy(dx, dy)
        hitTest(drag)
      }
    }
    requestAnimationFrame(tick)
  }
  requestAnimationFrame(tick)
}

/** A file drag from another program, as the backend reports it (physical pixels). */
export interface FileDragEvent {
  phase: 'enter' | 'over' | 'drop' | 'leave'
  x: number
  y: number
  files: readonly string[]
  folders: readonly string[]
  dropId: number | null
}

/** Follows a file drag from another program. */
export function fileDrag(event: FileDragEvent): void {
  const current = dragStore.get()
  if (event.phase === 'leave') {
    if (current?.payload.kind === 'files') {
      finish(null)
    }
    return
  }
  const x = event.x / window.devicePixelRatio
  const y = event.y / window.devicePixelRatio
  const payload =
    event.phase === 'over' && current?.payload.kind === 'files'
      ? current.payload
      : ({
          kind: 'files',
          files: event.files,
          folders: event.folders,
          dropId: event.dropId,
        } as const)
  if (payload.files.length === 0 && payload.folders.length === 0) {
    return
  }
  const drag: Drag = { payload, x, y, copy: false }
  update(drag)
  if (event.phase === 'drop') {
    finish(drag)
  }
}
