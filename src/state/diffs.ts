/**
 * Diff tabs: what each compares, and the documents it keeps open.
 *
 * A diff names two sides, each a file's absolute path or an address an
 * extension provides (`git:HEAD:src/main.rs`). A side not open yet opens
 * hidden (see `ipc/editor.ts`): without a tab of its own. Its documents stay
 * open while a diff tab shows them, so closing a document's own tab then
 * only hides it, and they close with the last diff tab showing them, which
 * asks to save unsaved changes first.
 */
import * as ipc from '../ipc/editor'
import type { DocumentSummary } from '../ipc/editor'
import { activeEditor, type EditorController } from '../editor/activeEditor'
import { documentsStore, setKeptOpen } from './documents'
import { reportError } from './notifications'
import { createStore } from './store'

export interface DiffSpec {
  /** Shown on the left, or as removed lines. */
  original: string
  /** Shown on the right, or as inserted lines; editable when it's a file. */
  modified: string
  title: string
}

export type DiffState =
  | { status: 'loading' }
  | { status: 'ready'; original: DocumentSummary; modified: DocumentSummary; dirty: boolean }
  | { status: 'failed'; message: string }

/** The diff tabs whose documents are open (or opening), by item. */
export const diffsStore = createStore<Readonly<Record<string, DiffState>>>({})

/** A diff's tab: the sides and title, so the same diff shows in one tab. */
export const diffItem = ({ original, modified, title }: DiffSpec): string =>
  `diff:${JSON.stringify([original, modified, title])}`

/** What a diff tab compares; `null` if `item` isn't a diff's. */
export function parseDiffItem(item: string): DiffSpec | null {
  if (!item.startsWith('diff:')) {
    return null
  }
  try {
    const parsed: unknown = JSON.parse(item.slice('diff:'.length))
    if (
      Array.isArray(parsed) &&
      parsed.length === 3 &&
      parsed.every(part => typeof part === 'string')
    ) {
      const [original, modified, title] = parsed as [string, string, string]
      return { original, modified, title }
    }
  } catch {
    // Not one of ours.
  }
  return null
}

/**
 * Views numbered from here are diff tabs': the backend doesn't remember
 * their folds, which hide unchanged lines (`DIFF_VIEWS` in
 * `src-tauri/src/editor/mod.rs`).
 */
const DIFF_VIEWS = 2 ** 30
let nextView = DIFF_VIEWS
const views = new Map<string, { original: number; modified: number }>()

/** The views a diff tab shows its documents in, the same each time it's shown. */
export function diffViews(item: string): { original: number; modified: number } {
  let pair = views.get(item)
  if (!pair) {
    pair = { original: nextView, modified: nextView + 1 }
    nextView += 2
    views.set(item, pair)
  }
  return pair
}

/** Shows the diff tab comparing `spec`'s sides, opening one if needed. */
export function openDiff(spec: DiffSpec): void {
  diffTabHandler?.(diffItem(spec))
}

let diffTabHandler: ((item: string) => void) | null = null

/** Set by the dock, which shows diff tabs. */
export function setDiffTabHandler(handler: (item: string) => void): void {
  diffTabHandler = handler
}

/** Whether a side is an address an extension provides, rather than a file. */
export const isAddress = (side: string): boolean => /^[a-z][a-z0-9+.-]{1,31}:/.test(side)

const openSide = (side: string) =>
  isAddress(side) ? ipc.openProvidedDocument(side, true) : ipc.openDocument(side, false, true)

function update(item: string, state: DiffState | null): void {
  diffsStore.update(diffs => {
    const { [item]: _, ...rest } = diffs
    return state ? { ...rest, [item]: state } : rest
  })
}

/** Opens a diff tab's documents, unless they're open or opening. */
export async function loadDiff(item: string): Promise<void> {
  const spec = parseDiffItem(item)
  if (!spec || diffsStore.get()[item]) {
    return
  }
  update(item, { status: 'loading' })
  const [original, modified] = await Promise.allSettled([
    openSide(spec.original),
    openSide(spec.modified),
  ])
  const opened = [original, modified].flatMap(side =>
    side.status === 'fulfilled' ? [side.value.id] : [],
  )
  const failure = [original, modified].find(side => side.status === 'rejected')
  if (original.status === 'fulfilled' && modified.status === 'fulfilled') {
    if (diffsStore.get()[item]?.status === 'loading') {
      update(item, {
        status: 'ready',
        original: original.value,
        modified: modified.value,
        dirty: modified.value.dirty,
      })
      return
    }
  } else if (failure?.status === 'rejected') {
    const error: unknown = failure.reason
    update(item, {
      status: 'failed',
      message: error instanceof Error ? error.message : String(error),
    })
  }
  // The other side, or both if the tab closed meanwhile, close again.
  await closeUnused(opened).catch(() => false)
}

/** How many times each document was reloaded, for inline diffs, which don't show the original. */
export const reloadsStore = createStore<Readonly<Record<number, number>>>({})

/** A document's text was replaced (its file changed, or its extension provided it again). */
export function noteReloaded(id: number): void {
  if (diffShows(id)) {
    reloadsStore.update(reloads => ({ ...reloads, [id]: (reloads[id] ?? 0) + 1 }))
  }
}

/** Notes whether a diff's modified document has unsaved changes, for its tab. */
export function setDiffDirty(item: string, dirty: boolean): void {
  const state = diffsStore.get()[item]
  if (state?.status === 'ready' && state.dirty !== dirty) {
    update(item, { ...state, dirty })
  }
}

/** Whether a diff tab shows document `id`. */
export function diffShows(id: number, except?: string): boolean {
  return Object.entries(diffsStore.get()).some(
    ([item, state]) =>
      item !== except &&
      state.status === 'ready' &&
      (state.original.id === id || state.modified.id === id),
  )
}

const listed = (id: number) => documentsStore.get().documents.some(document => document.id === id)

/** Closes the documents among `ids` that no tab shows. Resolves to false if one stayed open. */
async function closeUnused(ids: number[], except?: string): Promise<boolean> {
  for (const id of new Set(ids)) {
    if (!listed(id) && !diffShows(id, except) && !(await ipc.closeDocument(id))) {
      return false
    }
  }
  return true
}

/**
 * A diff tab is closing: closes its documents that no other tab shows,
 * asking to save unsaved changes. Resolves to false if the user cancelled,
 * when the tab stays.
 */
export async function releaseDiff(item: string): Promise<boolean> {
  const state = diffsStore.get()[item]
  if (state?.status === 'ready') {
    const pair = views.get(item)
    if (!(await closeUnused([state.modified.id, state.original.id], item))) {
      return false
    }
    // The documents other tabs show forget this tab's views.
    if (pair) {
      for (const [id, view] of [
        [state.original.id, pair.original],
        [state.modified.id, pair.modified],
      ] as const) {
        ipc.closeView(id, view).catch(() => {
          // Closed with the document.
        })
      }
    }
  }
  views.delete(item)
  update(item, null)
  return true
}

/** The diff tab whose modified side editor `editor` is, if any. */
function diffOf(editor: EditorController): { original: number; modified: number } | null {
  for (const [item, state] of Object.entries(diffsStore.get())) {
    if (
      state.status === 'ready' &&
      state.modified.id === editor.documentId &&
      views.get(item)?.modified === editor.view
    ) {
      return { original: state.original.id, modified: state.modified.id }
    }
  }
  return null
}

/**
 * Reverts the changes the active editor's selections touch: in a diff tab,
 * to its original side; elsewhere, to the file's original (`quickDiff`).
 */
export async function revertSelectedRanges(): Promise<void> {
  const editor = activeEditor.get()
  if (!editor) {
    return
  }
  try {
    const diff = diffOf(editor)
    await (diff
      ? ipc.diffRevert(diff.original, diff.modified, editor.view)
      : ipc.quickDiffRevert(editor.documentId, editor.view, null))
  } catch (error) {
    reportError('Reverting the changes', error)
  }
}

/** Applies (stages) the changes the active editor's selections touch to its original. */
export async function applySelectedRanges(): Promise<void> {
  const editor = activeEditor.get()
  if (!editor) {
    return
  }
  try {
    const diff = diffOf(editor)
    await (diff
      ? ipc.diffApply(diff.original, diff.modified, editor.view)
      : ipc.quickDiffApply(editor.documentId, editor.view, null))
  } catch (error) {
    reportError('Applying the changes', error)
  }
}

/** A diff tab's documents and the view of its modified side, once open. */
function diffSides(item: string): { original: number; modified: number; view: number } | null {
  const state = diffsStore.get()[item]
  return state?.status === 'ready'
    ? { original: state.original.id, modified: state.modified.id, view: diffViews(item).modified }
    : null
}

/** In a diff tab: reverts the changes its modified side's selections touch. */
export async function revertDiffRanges(item: string): Promise<void> {
  const sides = diffSides(item)
  try {
    if (sides) {
      await ipc.diffRevert(sides.original, sides.modified, sides.view)
    }
  } catch (error) {
    reportError('Reverting the changes', error)
  }
}

/** In a diff tab: applies (stages) the changes its modified side's selections touch. */
export async function applyDiffRanges(item: string): Promise<void> {
  const sides = diffSides(item)
  try {
    if (sides) {
      await ipc.diffApply(sides.original, sides.modified, sides.view)
    }
  } catch (error) {
    reportError('Applying the changes', error)
  }
}

/** Moves between a diff tab's changes, for the commands. Set by the tab while it shows. */
export interface DiffControls {
  goToChange(backward: boolean): void
}

const controls = new Map<string, DiffControls>()

export function setDiffControls(item: string, control: DiffControls | null): void {
  if (control) {
    controls.set(item, control)
  } else {
    controls.delete(item)
  }
}

export function diffControls(item: string): DiffControls | null {
  return controls.get(item) ?? null
}

setKeptOpen(id => diffShows(id))
