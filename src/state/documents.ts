/**
 * Open documents as the workbench sees them: summaries for tabs (the text
 * itself stays in the backend) and which document is active.
 */
import * as ipc from "../ipc/editor";
import type { DocumentSummary } from "../ipc/editor";
import { reportError } from "./notifications";
import { createStore } from "./store";

export interface DocumentsState {
  documents: readonly DocumentSummary[];
  activeId: number | null;
}

export const documentsStore = createStore<DocumentsState>({ documents: [], activeId: null });

/**
 * Hooks for work that must finish before a document is saved, such as edits
 * still in flight from the editor view. Registered by editor sessions.
 */
const beforeSave = new Map<number, () => Promise<void>>();

export function registerBeforeSave(id: number, flush: () => Promise<void>): () => void {
  beforeSave.set(id, flush);
  return () => {
    if (beforeSave.get(id) === flush) {
      beforeSave.delete(id);
    }
  };
}

function upsert(summary: DocumentSummary, activate: boolean): void {
  documentsStore.update(({ documents, activeId }) => {
    const exists = documents.some((d) => d.id === summary.id);
    return {
      documents: exists
        ? documents.map((d) => (d.id === summary.id ? summary : d))
        : [...documents, summary],
      activeId: activate ? summary.id : activeId,
    };
  });
}

function activeId(): number | null {
  return documentsStore.get().activeId;
}

/** Restores tabs for documents the backend already has open (e.g. after a reload). */
export async function loadDocuments(): Promise<void> {
  try {
    const documents = await ipc.listDocuments();
    documentsStore.set({ documents, activeId: documents.at(-1)?.id ?? null });
  } catch (error) {
    reportError("Loading open documents", error);
  }
}

export function activate(id: number): void {
  documentsStore.update((state) => ({ ...state, activeId: id }));
}

export async function newFile(): Promise<void> {
  try {
    upsert(await ipc.newDocument(), true);
  } catch (error) {
    reportError("Creating a file", error);
  }
}

export async function openFile(): Promise<void> {
  try {
    const summary = await ipc.openDocumentDialog();
    if (summary) {
      upsert(summary, true);
    }
  } catch (error) {
    reportError("Opening the file", error);
  }
}

export async function save(id = activeId(), as = false): Promise<void> {
  if (id === null) {
    return;
  }
  try {
    await beforeSave.get(id)?.();
    const summary = await (as ? ipc.saveDocumentAs(id) : ipc.saveDocument(id));
    if (summary) {
      upsert(summary, false);
    }
  } catch (error) {
    reportError("Saving", error);
  }
}

export async function close(id = activeId()): Promise<void> {
  if (id === null) {
    return;
  }
  try {
    await beforeSave.get(id)?.();
    if (!(await ipc.closeDocument(id))) {
      return;
    }
  } catch (error) {
    reportError("Closing the file", error);
    return;
  }
  documentsStore.update(({ documents, activeId }) => {
    const index = documents.findIndex((d) => d.id === id);
    const remaining = documents.filter((d) => d.id !== id);
    const neighbour = remaining[Math.min(index, remaining.length - 1)];
    return {
      documents: remaining,
      activeId: activeId === id ? (neighbour?.id ?? null) : activeId,
    };
  });
}

/** Records a dirty-state change reported by the editor view. */
export function setDirty(id: number, dirty: boolean): void {
  const current = documentsStore.get().documents.find((d) => d.id === id);
  if (current && current.dirty !== dirty) {
    upsert({ ...current, dirty }, false);
  }
}
