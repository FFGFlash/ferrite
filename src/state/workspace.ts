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

/**
 * Creates a file or folder in `parent` and expands the folders down to it,
 * returning its path. Failures are thrown for the caller to show.
 */
export async function createEntry(
  parent: string,
  name: string,
  directory: boolean,
): Promise<string> {
  const created = await ipc.createEntry(parent, name, directory)
  const root = workspaceStore.get().info?.root
  const open = expanded()
  for (const path of created.directories) {
    if (path !== root) {
      open.add(path)
    }
  }
  updateLayout({ expanded: [...open] })
  // Re-list now rather than waiting for the file watcher.
  await Promise.all(created.directories.map(load))
  return created.path
}

/** The folders between the workspace root and `path` (see `foldersBetween`). */
export function foldersTo(path: string): string[] | null {
  const root = workspaceStore.get().info?.root
  return root ? foldersBetween(root, path) : null
}

/** The separator in a path from the backend, which uses the platform's form. */
export function pathSeparator(path: string): string {
  return path.includes('\\') ? '\\' : '/'
}

/**
 * The folders between `root` and `path`, outermost first and not counting
 * `root`; `null` if `path` isn't inside `root`.
 */
export function foldersBetween(root: string, path: string): string[] | null {
  const separator = pathSeparator(root)
  const prefix = root.endsWith(separator) ? root : root + separator
  if (!path.startsWith(prefix) || path.length === prefix.length) {
    return null
  }
  const names = path.slice(prefix.length).split(separator).slice(0, -1)
  return names.map((_, index) => prefix + names.slice(0, index + 1).join(separator))
}

/**
 * Expands the folders down to `path` (a file or folder in the workspace)
 * and loads their listings. Returns whether `path` is in the workspace.
 */
export async function revealPath(path: string): Promise<boolean> {
  const folders = foldersTo(path)
  if (!folders) {
    return false
  }
  const open = expanded()
  if (folders.some(folder => !open.has(folder))) {
    updateLayout({ expanded: [...new Set([...open, ...folders])] })
  }
  const { listings } = workspaceStore.get()
  await Promise.all(folders.filter(folder => !listings.has(folder)).map(load))
  return true
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
