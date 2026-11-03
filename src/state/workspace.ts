/**
 * The open folder and the explorer tree. Directory listings are fetched
 * lazily when a folder is expanded and refreshed when the backend reports
 * changes inside it.
 */
import * as ipc from '../ipc/workspace'
import type { DirectoryEntry, WorkspaceInfo } from '../ipc/workspace'
import { layoutStore, updateLayout } from './layout'
import { showView } from './viewRequests'
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

/** Whether the user trusts the open folder changed; its listings stay. */
export function setWorkspaceTrustInfo(info: WorkspaceInfo): void {
  workspaceStore.update(state => (state.info?.root === info.root ? { ...state, info } : state))
}

export async function loadWorkspace(): Promise<void> {
  try {
    setWorkspace(await ipc.getWorkspace())
  } catch (error) {
    reportError('Loading the workspace', error)
  }
}

/** Opens a folder the user picks, or `path` (a folder dropped on the window). */
export async function openFolder(path?: string): Promise<void> {
  try {
    const info = await (path === undefined ? ipc.openWorkspaceDialog() : ipc.openWorkspace(path))
    if (info) {
      updateLayout({ expanded: [] })
      showView('explorer')
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

/** Whether `path` is `folder` or inside it. */
export function isWithin(path: string, folder: string): boolean {
  return path === folder || path.startsWith(folder + pathSeparator(folder))
}

/** The folder that holds `path`. */
export function parentOf(path: string): string {
  const index = path.lastIndexOf(pathSeparator(path))
  return index > 0 ? path.slice(0, index) : path
}

/** Where `path` is after `from` (a file or folder) moved to `to`; `null` if it didn't move. */
export function movedPath(path: string, from: string, to: string): string | null {
  return isWithin(path, from) ? to + path.slice(from.length) : null
}

/**
 * Follows a file or folder that moved from `from` to `to`: expanded folders
 * stay expanded under their new paths, and the listings that changed are
 * fetched again.
 */
async function afterMove(from: string, to: string): Promise<void> {
  const before = layoutStore.get().expanded
  const after = before.map(path => movedPath(path, from, to) ?? path)
  if (after.some((path, index) => path !== before[index])) {
    updateLayout({ expanded: after })
  }
  forget(from)
  const moved = after.filter(path => isWithin(path, to))
  await Promise.all([parentOf(from), parentOf(to), ...moved].map(reload))
}

/** Drops the listings of `path` and what's inside it, which no longer exist there. */
function forget(path: string): void {
  workspaceStore.update(state => {
    const listings = new Map(state.listings)
    for (const listed of state.listings.keys()) {
      if (isWithin(listed, path)) {
        listings.delete(listed)
      }
    }
    return { ...state, listings }
  })
}

/** Lists `path` again if it's loaded, or expanded (as a moved folder is). */
function reload(path: string): Promise<void> {
  const { listings, info } = workspaceStore.get()
  return listings.has(path) || path === info?.root || expanded().has(path)
    ? load(path)
    : Promise.resolve()
}

/**
 * Renames a file or folder, returning its new path; `name` may name new
 * subfolders, which are expanded. Failures are thrown for the caller to
 * show.
 */
export async function renameEntry(path: string, name: string): Promise<string> {
  const renamed = await ipc.renameEntry(path, name)
  const root = workspaceStore.get().info?.root
  const created = renamed.directories.filter(folder => folder !== root && !expanded().has(folder))
  if (created.length > 0) {
    updateLayout({ expanded: [...expanded(), ...created] })
  }
  await afterMove(path, renamed.path)
  await Promise.all(renamed.directories.map(reload))
  return renamed.path
}

/** Moves files and folders into `folder`, returning their new paths. */
export async function moveEntries(paths: readonly string[], folder: string): Promise<string[]> {
  const moved = await ipc.moveEntries([...paths], folder)
  await Promise.all(paths.map((path, index) => afterMove(path, moved[index] ?? path)))
  return moved
}

/** Copies files and folders into `folder`, returning the copies' paths. */
export async function copyEntries(paths: readonly string[], folder: string): Promise<string[]> {
  const copies = await ipc.copyEntries([...paths], folder)
  await reload(folder)
  return copies
}

/**
 * Copies the files and folders of a drop from another program into
 * `folder`, returning the copies' paths. The backend copies what was dropped
 * (`dropId` names the drop), so nothing outside the open folder can be named.
 */
export async function copyDropped(dropId: number, folder: string): Promise<string[]> {
  const copies = await ipc.copyDropped(dropId, folder)
  await reload(folder)
  return copies
}

/**
 * Deletes files and folders after the backend asks: to the trash, or
 * permanently. Returns false if the user cancelled.
 */
export async function deleteEntries(
  paths: readonly string[],
  permanent: boolean,
): Promise<boolean> {
  if (!(await ipc.deleteEntries([...paths], permanent))) {
    return false
  }
  const gone = (folder: string) => paths.some(path => isWithin(folder, path))
  if (layoutStore.get().expanded.some(gone)) {
    updateLayout({ expanded: layoutStore.get().expanded.filter(folder => !gone(folder)) })
  }
  paths.forEach(forget)
  await Promise.all(paths.map(path => reload(parentOf(path))))
  return true
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
