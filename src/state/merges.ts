/**
 * Merge tabs: the versions each combines, and the documents it keeps open.
 *
 * A merge names the versions' common base, the incoming and current
 * versions (each a file's absolute path or an address an extension
 * provides), and the result: the file being merged, edited in the tab. Its
 * documents open hidden, as a diff tab's do (see `diffs.ts`), and close
 * with the last tab showing them, which asks to save the result first.
 */
import * as ipc from '../ipc/editor'
import type { DocumentSummary, MergeDocuments } from '../ipc/editor'
import { runViewCommand, type MergeSpec } from '../ipc/extensions'
import { isAddress } from './diffs'
import { documentsStore, save, setKeptOpen } from './documents'
import { reportError } from './notifications'
import { createStore } from './store'

export type MergeState =
  | { status: 'loading' }
  | {
      status: 'ready'
      base: DocumentSummary
      incoming: DocumentSummary
      current: DocumentSummary
      result: DocumentSummary
      dirty: boolean
    }
  | { status: 'failed'; message: string }

/** The merge tabs whose documents are open (or opening), by item. */
export const mergesStore = createStore<Readonly<Record<string, MergeState>>>({})

/** A merge's tab: one per result file. */
export const mergeItem = (spec: MergeSpec): string => `merge:${JSON.stringify(spec)}`

const isSide = (value: unknown): boolean => {
  const side = value as Record<string, unknown> | null
  return (
    typeof side === 'object' &&
    side !== null &&
    typeof side.uri === 'string' &&
    typeof side.title === 'string' &&
    (side.description === null || typeof side.description === 'string')
  )
}

/** What a merge tab combines; `null` if `item` isn't a merge's. */
export function parseMergeItem(item: string): MergeSpec | null {
  if (!item.startsWith('merge:')) {
    return null
  }
  try {
    const parsed = JSON.parse(item.slice('merge:'.length)) as Record<string, unknown> | null
    if (
      parsed &&
      typeof parsed.base === 'string' &&
      isSide(parsed.incoming) &&
      isSide(parsed.current) &&
      typeof parsed.result === 'string' &&
      typeof parsed.title === 'string' &&
      typeof parsed.resetResult === 'boolean' &&
      (parsed.onComplete === null || typeof parsed.onComplete === 'string')
    ) {
      return parsed as unknown as MergeSpec
    }
  } catch {
    // Not one of ours.
  }
  return null
}

/** The documents of a ready merge. */
export const mergeDocuments = (
  state: Extract<MergeState, { status: 'ready' }>,
): MergeDocuments => ({
  base: state.base.id,
  incoming: state.incoming.id,
  current: state.current.id,
  result: state.result.id,
})

/**
 * Views numbered from here are merge tabs'. Like diff tabs' (from 2^30),
 * their folds aren't remembered.
 */
const MERGE_VIEWS = 2 ** 30 + 2 ** 29
let nextView = MERGE_VIEWS
export interface MergeViews {
  incoming: number
  current: number
  result: number
}
const views = new Map<string, MergeViews>()

/** The views a merge tab shows its documents in, the same each time it's shown. */
export function mergeViews(item: string): MergeViews {
  let found = views.get(item)
  if (!found) {
    found = { incoming: nextView, current: nextView + 1, result: nextView + 2 }
    nextView += 3
    views.set(item, found)
  }
  return found
}

let mergeTabHandler: ((item: string) => void) | null = null

/** Set by the dock, which shows merge tabs. */
export function setMergeTabHandler(handler: (item: string) => void): void {
  mergeTabHandler = handler
}

/** Shows the merge tab for `spec`'s result, replacing one for it with other versions. */
export function openMerge(spec: MergeSpec): void {
  mergeTabHandler?.(mergeItem(spec))
}

const openSide = (side: string) =>
  isAddress(side) ? ipc.openProvidedDocument(side, true) : ipc.openDocument(side, false, true)

function update(item: string, state: MergeState | null): void {
  mergesStore.update(merges => {
    const { [item]: _, ...rest } = merges
    return state ? { ...rest, [item]: state } : rest
  })
}

/** Result documents started again from the base, once each. */
const reset = new Set<number>()

/** Opens a merge tab's documents, unless they're open or opening. */
export async function loadMerge(item: string): Promise<void> {
  const spec = parseMergeItem(item)
  if (!spec || mergesStore.get()[item]) {
    return
  }
  update(item, { status: 'loading' })
  const sides = await Promise.allSettled([
    openSide(spec.base),
    openSide(spec.incoming.uri),
    openSide(spec.current.uri),
    ipc.openDocument(spec.result, false, true),
  ])
  const opened = sides.flatMap(side => (side.status === 'fulfilled' ? [side.value.id] : []))
  const failure = sides.find(side => side.status === 'rejected')
  if (!failure && mergesStore.get()[item]?.status === 'loading') {
    const [base, incoming, current, result] = sides.map(
      side => (side as PromiseFulfilledResult<DocumentSummary>).value,
    ) as [DocumentSummary, DocumentSummary, DocumentSummary, DocumentSummary]
    const ready = {
      status: 'ready' as const,
      base,
      incoming,
      current,
      result,
      dirty: result.dirty,
    }
    if (spec.resetResult && !reset.has(result.id)) {
      reset.add(result.id)
      try {
        if (await ipc.resetMerge(mergeDocuments(ready))) {
          ready.dirty = true
        }
      } catch (error) {
        reportError('Starting the merge', error)
      }
    }
    update(item, ready)
    return
  }
  if (failure?.status === 'rejected') {
    const error: unknown = failure.reason
    update(item, {
      status: 'failed',
      message: error instanceof Error ? error.message : String(error),
    })
  }
  await closeUnused(opened).catch(() => false)
}

/** Notes whether a merge's result has unsaved changes, for its tab. */
export function setMergeDirty(item: string, dirty: boolean): void {
  const state = mergesStore.get()[item]
  if (state?.status === 'ready' && state.dirty !== dirty) {
    update(item, { ...state, dirty })
  }
}

/** Whether a merge tab shows document `id`. */
export function mergeShows(id: number, except?: string): boolean {
  return Object.entries(mergesStore.get()).some(
    ([item, state]) =>
      item !== except &&
      state.status === 'ready' &&
      [state.base, state.incoming, state.current, state.result].some(
        document => document.id === id,
      ),
  )
}

const listed = (id: number) => documentsStore.get().documents.some(document => document.id === id)

async function closeUnused(ids: number[], except?: string): Promise<boolean> {
  // The result first: it may ask to save.
  for (const id of [...new Set(ids)].reverse()) {
    if (!listed(id) && !mergeShows(id, except) && !(await ipc.closeDocument(id))) {
      return false
    }
  }
  return true
}

/**
 * A merge tab is closing: closes its documents no other tab shows, asking
 * to save the result. Resolves to false if the user cancelled.
 */
export async function releaseMerge(item: string): Promise<boolean> {
  const state = mergesStore.get()[item]
  if (state?.status === 'ready') {
    const ids = [state.base.id, state.incoming.id, state.current.id, state.result.id]
    if (!(await closeUnused(ids, item))) {
      return false
    }
    const shown = views.get(item)
    if (shown) {
      for (const [id, view] of [
        [state.incoming.id, shown.incoming],
        [state.current.id, shown.current],
        [state.result.id, shown.result],
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

let closeTab: ((item: string) => Promise<boolean>) | null = null

/** Set by the dock, which closes tabs. */
export function setMergeTabCloser(close: (item: string) => Promise<boolean>): void {
  closeTab = close
}

/**
 * Completes a merge: saves the result, tells the extension (Git stages
 * it), and closes the tab.
 */
export async function completeMerge(item: string): Promise<void> {
  const spec = parseMergeItem(item)
  const state = mergesStore.get()[item]
  if (!spec || state?.status !== 'ready') {
    return
  }
  if (!(await save(state.result.id)) || mergesStore.get()[item]?.status !== 'ready') {
    return
  }
  try {
    if (spec.onComplete) {
      await runViewCommand(spec.onComplete, spec.result, [])
    }
  } catch (error) {
    reportError('Completing the merge', error)
    return
  }
  await closeTab?.(item)
}

/** Moves between a merge tab's conflicts, for the commands. Set by the tab while it shows. */
export interface MergeControls {
  goToConflict(backward: boolean): void
}

const controls = new Map<string, MergeControls>()

export function setMergeControls(item: string, control: MergeControls | null): void {
  if (control) {
    controls.set(item, control)
  } else {
    controls.delete(item)
  }
}

export function mergeControls(item: string): MergeControls | null {
  return controls.get(item) ?? null
}

setKeptOpen(id => mergeShows(id))
