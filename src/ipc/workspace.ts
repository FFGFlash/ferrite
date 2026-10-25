/**
 * Typed wrappers for the workspace, settings, session-state, and event
 * commands (`src-tauri/src/{workspace,settings,ui_state,events}.rs`).
 */
import { Channel } from '@tauri-apps/api/core'
import type { DocumentSummary, Position, SearchQuery } from './editor'
import { call, isDesktop } from './index'
import type { MessageLevel } from './extensions'

export interface WorkspaceInfo {
  root: string
  name: string
}

export interface DirectoryEntry {
  name: string
  path: string
  directory: boolean
  /** Matched by .gitignore: shown dimmed. */
  ignored: boolean
}

export interface FileResult {
  path: string
  relative: string
  /** UTF-16 indices of matched characters in `relative`. */
  indices: number[]
}

export interface FindFilesResult {
  files: FileResult[]
  indexing: boolean
  truncated: boolean
}

export interface LineMatch {
  line: number
  startColumn: number
  endColumn: number
  preview: string
  previewStart: number
  previewEnd: number
}

export type SearchEvent =
  | { type: 'file'; path: string; relative: string; matches: LineMatch[] }
  | {
      type: 'done'
      filesSearched: number
      filesWithMatches: number
      matches: number
      truncated: boolean
      cancelled: boolean
    }

/** `system`, `dark`, `light`, or a contributed theme's id. */
export type Theme = string

export interface FrontendSettings {
  theme: Theme
  /** Theme color overrides: token (without `--color-`) to hex color. */
  colors: Record<string, string>
  /** Command id to keys, replacing the defaults; empty removes them. */
  keybindings: Record<string, string[]>
  fontSize: number
  fontFamily: string
  tabSize: number
}

export interface SavedUi {
  layout: Record<string, unknown>
  activeFile: string | null
}

export type AppEvent =
  | { type: 'workspaceChanged'; workspace: WorkspaceInfo | null }
  | { type: 'filesChanged'; directories: string[] }
  | { type: 'documentReloaded'; document: DocumentSummary }
  | { type: 'documentChangedOnDisk'; document: DocumentSummary }
  | { type: 'settingsChanged'; settings: FrontendSettings }
  | { type: 'searchCounted'; documentId: number }
  | { type: 'syntaxUpdated'; documentId: number }
  | { type: 'diagnosticsChanged'; documentId: number }
  | { type: 'languageServerChanged' }
  | { type: 'documentOpened'; document: DocumentSummary; active: boolean }
  | { type: 'indexStatus'; indexing: boolean; files: number; truncated: boolean }
  | { type: 'extensionsChanged' }
  | { type: 'extensionMessage'; extension: string; level: MessageLevel; message: string }
  | { type: 'extensionFailed'; id: string; name: string; reason: string }
  | { type: 'documentEdited'; documentId: number }
  | { type: 'extensionStatusItemsChanged' }
  | { type: 'extensionViewChanged'; view: string }
  | { type: 'extensionShowView'; view: string }
  | { type: 'decorationsChanged'; documentId: number }
  | { type: 'extensionReveal'; documentId: number; start: Position; end: Position }
  | { type: 'notice'; level: 'warning'; message: string }

export const getWorkspace = () => call<WorkspaceInfo | null>('workspace_get')
export const openWorkspaceDialog = () => call<WorkspaceInfo | null>('workspace_open_dialog')
export const closeWorkspace = () => call<void>('workspace_close')
export const listDirectory = (path: string) =>
  call<DirectoryEntry[]>('workspace_list_dir', { path })
export const findFiles = (query: string) => call<FindFilesResult>('workspace_find_files', { query })
export const cancelSearch = () => call<void>('workspace_search_cancel')

/** Starts a find-in-files search; `onEvent` receives results as they arrive. */
export function searchWorkspace(
  query: SearchQuery,
  onEvent: (event: SearchEvent) => void,
): Promise<void> {
  const channel = new Channel<SearchEvent>()
  channel.onmessage = onEvent
  return call<void>('workspace_search', { query, channel })
}

export const getSettings = () => call<FrontendSettings>('settings_get')
export const settingsFilePath = () => call<string>('settings_file')
export const setTheme = (theme: Theme) => call<void>('settings_set_theme', { theme })

export const getSavedUi = () => call<SavedUi>('ui_state_get')
export const saveUi = (layout: Record<string, unknown>, activeFile: string | null) =>
  call<void>('ui_state_save', { layout, activeFile })

/** Subscribes to backend notifications. Does nothing outside the desktop app. */
export function subscribeToEvents(onEvent: (event: AppEvent) => void): void {
  if (!isDesktop()) {
    return
  }
  const channel = new Channel<AppEvent>()
  channel.onmessage = onEvent
  void call<void>('events_subscribe', { channel })
}
