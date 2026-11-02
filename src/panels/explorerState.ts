/**
 * Explorer state shared with its header actions, commands, and the editor's
 * breadcrumbs: the focused row, the file or folder being named or renamed,
 * what was cut or copied, and the last row revealed. The explorer reveals
 * the active file as it changes.
 *
 * The actions here (rename, delete, cut, copy, paste, move, copy path,
 * reveal) are shared by the context menu, the tree's keys, drag and drop,
 * and commands. They report their own failures.
 */
import { isMac } from '../editor/keymap'
import type { DirectoryEntry } from '../ipc/workspace'
import { revealEntry, writeClipboardText } from '../ipc/workspace'
import { documentsStore } from '../state/documents'
import { layoutStore } from '../state/layout'
import { reportError } from '../state/notifications'
import { createStore } from '../state/store'
import {
  copyDropped,
  copyEntries,
  deleteEntries,
  isWithin,
  moveEntries,
  parentOf,
  pathSeparator,
  renameEntry,
  revealPath,
  toggleFolder,
  workspaceStore,
  type Listing,
} from '../state/workspace'

/** The explorer tree's element id, for moving focus to it. */
export const EXPLORER_TREE_ID = 'explorer-tree'

export interface PendingCreate {
  /** The folder the new entry goes in. */
  parent: string
  directory: boolean
}

/** Files and folders cut or copied, to paste into a folder. */
export interface ExplorerClipboard {
  paths: string[]
  /** Cut: pasting moves them. */
  cut: boolean
}

export interface ExplorerState {
  focused: string | null
  creating: PendingCreate | null
  /** The file or folder being renamed. */
  renaming: string | null
  clipboard: ExplorerClipboard | null
  /** A row to scroll into view once it is listed; `seq` tells requests apart. */
  revealed: { path: string; seq: number } | null
}

export const explorerStore = createStore<ExplorerState>({
  focused: null,
  creating: null,
  renaming: null,
  clipboard: null,
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
  const parent = folderFor(target ?? explorerStore.get().focused)
  if (!parent) {
    return
  }
  const root = workspaceStore.get().info?.root
  if (parent !== root && !layoutStore.get().expanded.includes(parent)) {
    toggleFolder(parent)
  }
  explorerStore.update(state => ({ ...state, renaming: null, creating: { parent, directory } }))
}

export function cancelCreate(): void {
  explorerStore.update(state => ({ ...state, creating: null }))
}

/** The folder `path` names, or the one holding the file it names; the root without one. */
function folderFor(path: string | null | undefined): string | null {
  const { info, listings } = workspaceStore.get()
  if (!info) {
    return null
  }
  const found = path ? findEntry(path, listings) : null
  if (!found) {
    return info.root
  }
  return found.entry.directory ? found.entry.path : found.parent
}

/** The focused row, if it's a file or folder in the tree. */
export function focusedEntry(): DirectoryEntry | null {
  const { focused } = explorerStore.get()
  return focused ? (findEntry(focused, workspaceStore.get().listings)?.entry ?? null) : null
}

/** Shows a name field in place of a file's or folder's name. */
export function beginRename(path = explorerStore.get().focused): void {
  if (path) {
    explorerStore.update(state => ({ ...state, creating: null, renaming: path, focused: path }))
  }
}

export function cancelRename(): void {
  explorerStore.update(state => ({ ...state, renaming: null }))
}

/**
 * Renames `path` to `name`, then focuses it under its new name. Failures
 * are thrown, so the name field can show them.
 */
export async function rename(path: string, name: string): Promise<string> {
  const renamed = await renameEntry(path, name)
  explorerStore.update(state => ({ ...state, renaming: null, focused: renamed }))
  await revealInExplorer(renamed)
  return renamed
}

/**
 * Asks, then deletes `path`: to the system's trash, or permanently. If the
 * focused row goes with it, `focusAfter` (the row that takes its place) is
 * focused instead.
 */
export async function deleteEntry(
  path: string,
  permanent = false,
  focusAfter: string | null = null,
): Promise<void> {
  try {
    if (await deleteEntries([path], permanent)) {
      explorerStore.update(state => ({
        ...state,
        focused:
          state.focused && isWithin(state.focused, path)
            ? (focusAfter ?? parentOf(path))
            : state.focused,
        clipboard: state.clipboard && {
          ...state.clipboard,
          paths: state.clipboard.paths.filter(other => !isWithin(other, path)),
        },
      }))
    }
  } catch (error) {
    reportError('Deleting', error)
  }
}

export function cutEntry(path: string): void {
  explorerStore.update(state => ({ ...state, clipboard: { paths: [path], cut: true } }))
}

export function copyEntry(path: string): void {
  explorerStore.update(state => ({ ...state, clipboard: { paths: [path], cut: false } }))
}

/** Whether there's something to paste. */
export function canPaste(): boolean {
  return (explorerStore.get().clipboard?.paths.length ?? 0) > 0
}

/**
 * Pastes what was cut (moving it) or copied into the folder `target`
 * names, or the folder holding the file it names.
 */
export async function paste(target?: string | null): Promise<void> {
  const { clipboard, focused } = explorerStore.get()
  const folder = folderFor(target === undefined ? focused : target)
  if (!clipboard || clipboard.paths.length === 0 || !folder) {
    return
  }
  const placed = await moveOrCopy(clipboard.paths, folder, !clipboard.cut)
  if (placed && clipboard.cut) {
    explorerStore.update(state => ({ ...state, clipboard: null }))
  }
}

/**
 * Moves (or copies) files and folders into `folder`, as dragging does,
 * then focuses the first. Returns whether it worked.
 */
export async function moveOrCopy(
  paths: readonly string[],
  folder: string,
  copy: boolean,
): Promise<boolean> {
  try {
    await focusPlaced(copy ? await copyEntries(paths, folder) : await moveEntries(paths, folder))
    return true
  } catch (error) {
    reportError(copy ? 'Copying' : 'Moving', error)
    return false
  }
}

/** Copies files and folders dropped from another program (drop `dropId`) into `folder`. */
export async function copyDroppedInto(dropId: number, folder: string): Promise<void> {
  try {
    await focusPlaced(await copyDropped(dropId, folder))
  } catch (error) {
    reportError('Copying', error)
  }
}

async function focusPlaced(placed: readonly string[]): Promise<void> {
  if (placed[0]) {
    setFocused(placed[0])
    await revealInExplorer(placed[0])
  }
}

/** Whether `paths` can be moved into `folder`: not into themselves, and somewhere new. */
export function canMoveInto(paths: readonly string[], folder: string): boolean {
  return (
    paths.length > 0 && paths.every(path => !isWithin(folder, path) && parentOf(path) !== folder)
  )
}

/** Copies a file's or folder's path, or its path relative to the open folder. */
export async function copyPath(path: string, relative: boolean): Promise<void> {
  const root = workspaceStore.get().info?.root
  const separator = pathSeparator(path)
  const text =
    relative && root && path.startsWith(root + separator) ? path.slice(root.length + 1) : path
  try {
    await writeClipboardText(text)
  } catch (error) {
    reportError('Copying the path', error)
  }
}

/** What showing a file in the system's file manager is called here. */
export function revealLabel(): string {
  if (isMac()) {
    return 'Reveal in Finder'
  }
  return navigator.userAgent.includes('Windows')
    ? 'Reveal in File Explorer'
    : 'Open Containing Folder'
}

/** Shows a file or folder in the system's file manager. */
export async function revealInSystem(path: string): Promise<void> {
  try {
    await revealEntry(path)
  } catch (error) {
    reportError('Showing it in the file manager', error)
  }
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
  const key = path && root ? `${root}\n${path}` : null
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
