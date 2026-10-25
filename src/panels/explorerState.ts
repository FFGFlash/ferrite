/**
 * Explorer state shared with its header actions and commands: the focused
 * row and the file or folder being named.
 */
import type { DirectoryEntry } from '../ipc/workspace'
import { layoutStore } from '../state/layout'
import { createStore } from '../state/store'
import { toggleFolder, workspaceStore, type Listing } from '../state/workspace'

export interface PendingCreate {
  /** The folder the new entry goes in. */
  parent: string
  directory: boolean
}

export interface ExplorerState {
  focused: string | null
  creating: PendingCreate | null
}

export const explorerStore = createStore<ExplorerState>({ focused: null, creating: null })

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
