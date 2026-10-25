/**
 * Explorer state shared with its header actions, commands, and the editor's
 * breadcrumbs: the focused row, the file or folder being named, and the
 * last row revealed. The explorer reveals the active file as it changes.
 */
import type { DirectoryEntry } from '../ipc/workspace'
import { documentsStore } from '../state/documents'
import { layoutStore } from '../state/layout'
import { createStore } from '../state/store'
import { revealPath, toggleFolder, workspaceStore, type Listing } from '../state/workspace'

/** The explorer tree's element id, for moving focus to it. */
export const EXPLORER_TREE_ID = 'explorer-tree'

export interface PendingCreate {
  /** The folder the new entry goes in. */
  parent: string
  directory: boolean
}

export interface ExplorerState {
  focused: string | null
  creating: PendingCreate | null
  /** A row to scroll into view once it is listed; `seq` tells requests apart. */
  revealed: { path: string; seq: number } | null
}

export const explorerStore = createStore<ExplorerState>({
  focused: null,
  creating: null,
  revealed: null,
})

export function setFocused(focused: string | null): void {
  explorerStore.update(state => ({ ...state, focused }))
}

/**
 * Shows a name field for a new file or folder: inside `target` if it is a
 * folder, beside it if it is a file. Without a target, the focused row is
 * used, and without one of those, the workspace root.
 */
export function beginCreate(directory: boolean, target?: string): void {
  const { info, listings } = workspaceStore.get()
  if (!info) {
    return
  }
  const path = target ?? explorerStore.get().focused
  let parent = info.root
  if (path) {
    const found = findEntry(path, listings)
    if (found) {
      parent = found.entry.directory ? found.entry.path : found.parent
    }
  }
  if (parent !== info.root && !layoutStore.get().expanded.includes(parent)) {
    toggleFolder(parent)
  }
  explorerStore.update(state => ({ ...state, creating: { parent, directory } }))
}

export function cancelCreate(): void {
  explorerStore.update(state => ({ ...state, creating: null }))
}

let revealSeq = 0

/**
 * Expands the folders down to `path`, makes it the focused row, and scrolls
 * it into view. Does nothing for paths outside the workspace.
 */
export async function revealInExplorer(path: string): Promise<void> {
  if (await revealPath(path)) {
    explorerStore.update(state => ({
      ...state,
      focused: path,
      revealed: { path, seq: ++revealSeq },
    }))
  }
}

/** The active file and workspace last revealed, so each is revealed once. */
let revealedActive: string | null = null

/** Reveals the active file when it (or the workspace) changes, as VS Code does. */
function revealActiveFile(): void {
  const { documents, activeId } = documentsStore.get()
  const path = documents.find(document => document.id === activeId)?.path ?? null
  const root = workspaceStore.get().info?.root ?? null
  const key =
    path && root
      ? `${root}
${path}`
      : null
  if (key !== revealedActive) {
    revealedActive = key
    if (path) {
      void revealInExplorer(path)
    }
  }
}

documentsStore.subscribe(revealActiveFile)
workspaceStore.subscribe(revealActiveFile)

/** A loaded entry and the folder that lists it. */
function findEntry(
  path: string,
  listings: ReadonlyMap<string, Listing>,
): { entry: DirectoryEntry; parent: string } | null {
  for (const [parent, listing] of listings) {
    const entry = 'error' in listing ? undefined : listing.find(item => item.path === path)
    if (entry) {
      return { entry, parent }
    }
  }
  return null
}
