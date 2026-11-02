/**
 * Handles on the editor views on screen, so commands (find, go to line, add
 * next match) and events can act on them without knowing about React.
 * Several editors can be visible at once (one per dock group); the active
 * editor is the one showing the active document.
 */
import type { EditorOp, Position } from '../ipc/editor'
import { createStore } from '../state/store'

/**
 * How an operation's cursor is scrolled into view: just enough (`nearest`,
 * as when typing), to the middle (`center`), or to the middle only when it
 * is off screen (`centerIfOutside`, as when navigating), as in VS Code.
 */
export type Reveal = 'nearest' | 'center' | 'centerIfOutside'

/** Whole lines highlighted and scrolled to without moving the cursor. */
export interface LinePreview {
  start: number
  end: number
  /** The line scrolled to the middle. */
  center: number
}

export interface ScrollPosition {
  top: number
  left: number
}

export interface EditorController {
  documentId: number
  /** Applies `op`, then reveals the cursor (`nearest` by default). */
  dispatch(op: EditorOp, reveal?: Reveal): void
  /** Opens the find widget, prefilled from a single-line selection. */
  openFind(replace: boolean): void
  /** Moves to the next or previous match of the find widget's query. */
  findNext(backward: boolean): void
  /** Re-fetches the view, e.g. after the document was reloaded from disk. */
  refresh(): void
  focus(): void
  lineCount(): number
  /** Scrolls so `line` is at the top of the viewport. */
  scrollToLine(line: number): void
  /** Scrolls by `lines` lines (negative for up) without moving the cursor. */
  scrollBy(lines: number): void
  /** Goes to the definition of the symbol at the cursor. */
  goToDefinition(): void
  /** Goes to the one reference to the symbol at the cursor, or peeks at several. */
  goToReferences(): void
  /** Peeks at the references to the symbol at the cursor. */
  peekReferences(): void
  /** Opens completion at the cursor. */
  triggerSuggest(): void
  /** Shows hover information for the symbol at the cursor. */
  showHover(): void
  /** The primary cursor, once the view has arrived. */
  cursor(): Position | null
  /** Highlights lines and scrolls to them (Go to Line and Go to Symbol), or stops. */
  previewLines(preview: LinePreview | null): void
  scrollPosition(): ScrollPosition
  /** Scrolls back to a position from `scrollPosition`. */
  restoreScroll(position: ScrollPosition): void
}

export const activeEditor = createStore<EditorController | null>(null)

/** Mounted editors by document; the last one mounted or focused wins. */
const mounted = new Map<number, EditorController>()
let activeDocument: number | null = null
let lastUsed: EditorController | null = null

function update(): void {
  const active = activeDocument === null ? undefined : mounted.get(activeDocument)
  activeEditor.set(
    active ?? (lastUsed && mounted.get(lastUsed.documentId) === lastUsed ? lastUsed : null),
  )
}

/** Registers a mounted editor view; returns the function that unregisters it. */
export function registerEditor(controller: EditorController): () => void {
  mounted.set(controller.documentId, controller)
  lastUsed = controller
  update()
  return () => {
    if (mounted.get(controller.documentId) === controller) {
      mounted.delete(controller.documentId)
    }
    if (lastUsed === controller) {
      lastUsed = null
    }
    update()
  }
}

/** The editor showing a document, if one is mounted. */
export function editorFor(documentId: number): EditorController | null {
  return mounted.get(documentId) ?? null
}

export function mountedEditors(): EditorController[] {
  return [...mounted.values()]
}

/** Called when the active document changes. */
export function setActiveDocument(id: number | null): void {
  activeDocument = id
  update()
}

/**
 * Selects `start..end` (or places the cursor at `start`) and scrolls to it,
 * to the middle of the editor if it's off screen.
 */
export function selectRange(
  editor: EditorController,
  start: Position,
  end = start,
  reveal: Reveal = 'centerIfOutside',
): void {
  editor.dispatch({ type: 'setCursor', position: start, extend: false, add: false }, reveal)
  if (end.line !== start.line || end.column !== start.column) {
    editor.dispatch({ type: 'setCursor', position: end, extend: true, add: false }, reveal)
  }
  editor.focus()
}
