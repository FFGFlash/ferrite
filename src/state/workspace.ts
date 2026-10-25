/**
 * The open folder and the explorer tree. Directory listings are fetched
 * lazily when a folder is expanded and refreshed when the backend reports
 * changes inside it.
 */
import * as ipc from '../ipc/workspace'
import type { DirectoryEntry, WorkspaceInfo } from '../ipc/workspace'
import { layoutStore, updateLayout } from './layout'
import { reportError } from './notifications'
import { createStore } from './store'

export type Listing = DirectoryEntry[] | { error: string }

export interface WorkspaceState {
  info: WorkspaceInfo | null
  /** Loaded directory listings by absolute path. */
  listings: ReadonlyMap<string, Listing>
  /** Whether the Go to File index is still being built. */
  indexing: boolean
}

export const workspaceStore = createStore<WorkspaceState>({
  info: null,
  listings: new Map(),
  indexing: false,
})

const expanded = () => new Set(layoutStore.get().expanded)

/** Sets the workspace and loads the root and previously expanded folders. */
export function setWorkspace(info: WorkspaceInfo | null): void {
  workspaceStore.set({ info, listings: new Map(), indexing: info !== null })
  if (info) {
    void load(info.root)
    for (const path of expanded()) {
      if (path.startsWith(info.root)) {
        void load(path)
      }
    }
  }
}

export async function loadWorkspace(): Promise<void> {
  try {
    setWorkspace(await ipc.getWorkspace())
  } catch (error) {
    reportError('Loading the workspace', error)
  }
}

export async function openFolder(): Promise<void> {
  try {
    const info = await ipc.openWorkspaceDialog()
    if (info) {
      updateLayout({ expanded: [], activeView: 'explorer', sidebarVisible: true })
      setWorkspace(info)
    }
  } catch (error) {
    reportError('Opening the folder', error)
  }
}

export async function closeFolder(): Promise<void> {
  try {
    await ipc.closeWorkspace()
    updateLayout({ expanded: [] })
    setWorkspace(null)
  } catch (error) {
    reportError('Closing the folder', error)
  }
}

async function load(path: string): Promise<void> {
  let listing: Listing
  try {
    listing = await ipc.listDirectory(path)
  } catch (error) {
    listing = { error: error instanceof Error ? error.message : String(error) }
  }
  workspaceStore.update(state => {
    if (!state.info || !path.startsWith(state.info.root)) {
      return state
    }
    const listings = new Map(state.listings)
    listings.set(path, listing)
    return { ...state, listings }
  })
}

export function toggleFolder(path: string): void {
  const open = expanded()
  if (open.has(path)) {
    open.delete(path)
  } else {
    open.add(path)
    if (!workspaceStore.get().listings.has(path)) {
      void load(path)
    }
  }
  updateLayout({ expanded: [...open] })
}

export function collapseAll(): void {
  updateLayout({ expanded: [] })
}

/** Re-lists loaded directories among `directories` (absolute paths). */
export function refreshDirectories(directories: readonly string[]): void {
  const { listings } = workspaceStore.get()
  for (const directory of directories) {
    if (listings.has(directory)) {
      void load(directory)
    }
  }
}

export function setIndexing(indexing: boolean): void {
  workspaceStore.update(state => ({ ...state, indexing }))
}
