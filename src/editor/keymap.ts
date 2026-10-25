/**
 * Key bindings inside the editor. Pure functions so platform differences can
 * be tested without a DOM.
 */
import type { ClipboardAction, EditorOp, Motion } from '../ipc/editor'

export interface KeyInput {
  key: string
  ctrlKey: boolean
  metaKey: boolean
  altKey: boolean
  shiftKey: boolean
}

export type EditorCommand =
  | { kind: 'op'; op: EditorOp }
  | { kind: 'clipboard'; action: ClipboardAction }
  /** Handled: prevent the browser default but do nothing. */
  | { kind: 'ignore' }

export const isMac = (): boolean => /Mac|iPhone|iPad/.test(navigator.userAgent)

const op = (value: EditorOp): EditorCommand => ({ kind: 'op', op: value })
const move = (motion: Motion, extend: boolean, pageLines?: number): EditorCommand =>
  op({ type: 'move', motion, extend, ...(pageLines ? { pageLines } : {}) })

/**
 * Maps a key press to an editor command, or `null` to let it through (for
 * example to the textarea for text input, or to workbench shortcuts).
 */
export function keyToCommand(e: KeyInput, mac: boolean, pageLines: number): EditorCommand | null {
  const mod = mac ? e.metaKey : e.ctrlKey
  const wordMod = mac ? e.altKey : e.ctrlKey
  const extend = e.shiftKey

  switch (e.key) {
    case 'ArrowLeft':
    case 'ArrowRight': {
      const left = e.key === 'ArrowLeft'
      if (mac && e.metaKey) {
        return move(left ? 'lineStart' : 'lineEnd', extend)
      }
      if (wordMod) {
        return move(left ? 'wordLeft' : 'wordRight', extend)
      }
      return move(left ? 'left' : 'right', extend)
    }
    case 'ArrowUp':
    case 'ArrowDown': {
      const up = e.key === 'ArrowUp'
      if (mod && e.altKey) {
        return op({ type: 'addCursor', direction: up ? 'up' : 'down' })
      }
      if (mac && e.metaKey) {
        return move(up ? 'documentStart' : 'documentEnd', extend)
      }
      if (e.ctrlKey || e.altKey || e.metaKey) {
        return null
      }
      return move(up ? 'up' : 'down', extend)
    }
    case 'Home':
    case 'End': {
      const home = e.key === 'Home'
      if (e.ctrlKey || e.metaKey) {
        return move(home ? 'documentStart' : 'documentEnd', extend)
      }
      return move(home ? 'lineStart' : 'lineEnd', extend)
    }
    case 'PageUp':
    case 'PageDown':
      return move(e.key === 'PageUp' ? 'pageUp' : 'pageDown', extend, pageLines)
    case 'Backspace':
    case 'Delete':
      return op({
        type: 'delete',
        direction: e.key === 'Backspace' ? 'backward' : 'forward',
        unit: wordMod ? 'word' : 'grapheme',
      })
    case 'Enter':
      return mod || e.altKey ? null : op({ type: 'newline' })
    case 'Tab':
      if (mod || e.altKey) {
        return null
      }
      // Outdenting is not implemented yet; keep focus in the editor.
      return extend ? { kind: 'ignore' } : op({ type: 'tab' })
    case 'Escape':
      return op({ type: 'collapseSelections' })
  }

  if (!mod || e.altKey) {
    return null
  }
  switch (e.key.toLowerCase()) {
    case 'a':
      return op({ type: 'selectAll' })
    case 'z':
      return op({ type: extend ? 'redo' : 'undo' })
    case 'y':
      return mac ? null : op({ type: 'redo' })
    case 'c':
      return { kind: 'clipboard', action: 'copy' }
    case 'x':
      return { kind: 'clipboard', action: 'cut' }
  }
  return null
}
