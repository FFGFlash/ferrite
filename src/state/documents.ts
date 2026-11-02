/**
 * Open documents as the workbench sees them: summaries for tabs (the text
 * itself stays in the backend) and which document is active.
 */
import { editorFor, selectRange, setActiveDocument } from '../editor/activeEditor'
import { CommandError } from '../ipc'
import * as ipc from '../ipc/editor'
import type { DocumentSummary, Position } from '../ipc/editor'
import { reportError } from './notifications'
import { refreshSettingsProblem } from './problems'
import { refreshSettingsIfLoaded } from './settings'
import { createStore } from './store'

export interface DocumentsState {
  documents: readonly DocumentSummary[]
  activeId: number | null
}

export const documentsStore = createStore<DocumentsState>({ documents: [], activeId: null })
documentsStore.subscribe(() => setActiveDocument(documentsStore.get().activeId))

/**
 * Hooks for work that must finish before a document is saved, such as edits
 * still in flight from the editor view. Registered by editor sessions.
 */
const beforeSave = new Map<number, () => Promise<void>>()

export function registerBeforeSave(id: number, flush: () => Promise<void>): () => void {
  beforeSave.set(id, flush)
  return () => {
    if (beforeSave.get(id) === flush) {
      beforeSave.delete(id)
    }
  }
}

/**
 * Selections to apply when a document's editor view next mounts, for
 * documents opened at a location (search results, Go to File).
 */
const pendingSelections = new Map<number, { start: Position; end: Position }>()

export function pendingSelection(id: number): { start: Position; end: Position } | undefined {
  return pendingSelections.get(id)
}

/** Marks a pending selection as applied (if it is still the pending one). */
export function clearPendingSelection(
  id: number,
  applied: { start: Position; end: Position },
): void {
  if (pendingSelections.get(id) === applied) {
    pendingSelections.delete(id)
  }
}

/** Updates a document's summary (for example after a reload from disk). */
export function updateSummary(summary: DocumentSummary): void {
  if (documentsStore.get().documents.some(d => d.id === summary.id)) {
    upsert(summary, false)
  }
}

function upsert(summary: DocumentSummary, activate: boolean): void {
  documentsStore.update(({ documents, activeId }) => {
    const exists = documents.some(d => d.id === summary.id)
    return {
      documents: exists
        ? documents.map(d => (d.id === summary.id ? summary : d))
        : [...documents, summary],
      activeId: activate ? summary.id : activeId,
    }
  })
}

function activeId(): number | null {
  return documentsStore.get().activeId
}

/** Restores tabs for documents the backend already has open (e.g. after a reload). */
export async function loadDocuments(activePath: string | null = null): Promise<void> {
  try {
    const documents = await ipc.listDocuments()
    const active = documents.find(d => d.path === activePath) ?? documents.at(-1)
    documentsStore.set({ documents, activeId: active?.id ?? null })
  } catch (error) {
    reportError('Loading open documents', error)
  }
}

export function activate(id: number): void {
  documentsStore.update(state => ({ ...state, activeId: id }))
}

/**
 * Opens (or switches to) the file at `path`, optionally selecting a range
 * once its editor is showing.
 */
export async function openPath(
  path: string,
  selection?: { start: Position; end?: Position },
): Promise<void> {
  try {
    const summary = await ipc.openDocument(path)
    const editor = editorFor(summary.id)
    if (selection) {
      const range = { start: selection.start, end: selection.end ?? selection.start }
      if (editor?.documentId === summary.id) {
        selectRange(editor, range.start, range.end)
      } else {
        pendingSelections.set(summary.id, range)
      }
    }
    upsert(summary, true)
  } catch (error) {
    reportError('Opening the file', error)
  }
}

/** Creates an untitled document, optionally containing `text`. */
export async function newFile(text?: string): Promise<void> {
  try {
    upsert(await ipc.newDocument(text), true)
  } catch (error) {
    reportError('Creating a file', error)
  }
}

/** Makes an open document active and selects `start..end` in it. */
export function revealInDocument(id: number, start: Position, end: Position): void {
  const editor = editorFor(id)
  if (editor) {
    activate(id)
    selectRange(editor, start, end)
    return
  }
  if (documentsStore.get().documents.some(document => document.id === id)) {
    pendingSelections.set(id, { start, end })
    activate(id)
  }
}

export async function openFile(): Promise<void> {
  try {
    const summary = await ipc.openDocumentDialog()
    if (summary) {
      upsert(summary, true)
    }
  } catch (error) {
    reportError('Opening the file', error)
  }
}

export async function save(id = activeId(), as = false): Promise<void> {
  if (id === null) {
    return
  }
  try {
    await beforeSave.get(id)?.()
    const summary = await (as ? ipc.saveDocumentAs(id) : ipc.saveDocument(id))
    if (summary) {
      upsert(summary, false)
    }
  } catch (error) {
    reportError('Saving', error)
    if (error instanceof CommandError && error.code === 'invalidSettings') {
      // settings.toml was saved with a mistake; the settings editor shows it.
      refreshSettingsIfLoaded()
      void refreshSettingsProblem()
    }
  }
}

/** Saves every document with unsaved changes, one at a time (untitled ones ask for a path). */
export async function saveAll(): Promise<void> {
  for (const document of documentsStore.get().documents.filter(d => d.dirty)) {
    await save(document.id)
  }
}

export async function close(id = activeId()): Promise<void> {
  if (id === null) {
    return
  }
  try {
    await beforeSave.get(id)?.()
    if (!(await ipc.closeDocument(id))) {
      return
    }
  } catch (error) {
    reportError('Closing the file', error)
    return
  }
  documentsStore.update(({ documents, activeId }) => {
    const index = documents.findIndex(d => d.id === id)
    const remaining = documents.filter(d => d.id !== id)
    const neighbour = remaining[Math.min(index, remaining.length - 1)]
    return {
      documents: remaining,
      activeId: activeId === id ? (neighbour?.id ?? null) : activeId,
    }
  })
}

/** Records a dirty-state change reported by the editor view. */
export function setDirty(id: number, dirty: boolean): void {
  const current = documentsStore.get().documents.find(d => d.id === id)
  if (current && current.dirty !== dirty) {
    upsert({ ...current, dirty }, false)
  }
}
