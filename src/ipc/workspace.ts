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

export interface CreatedEntry {
  path: string
  /** Folders whose listings gained an entry, outermost first. */
  directories: string[]
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
  /** `default` or a contributed icon theme's id. */
  iconTheme: string
  /** Theme color overrides: token (without `--color-`) to hex color. */
  colors: Record<string, string>
  /** Command id to keys, replacing the defaults; empty removes them. */
  keybindings: Record<string, string[]>
  fontSize: number
  fontFamily: string
  tabSize: number
  /** Lines kept above a terminal's screen. */
  terminalScrollback: number
  /** Whether terminals announce output to screen readers. */
  terminalScreenReader: boolean
  /** Whether the minimap labels named `#region` comments. */
  minimapRegionHeaders: boolean
  /** Whether the minimap labels `MARK:` comments. */
  minimapMarkHeaders: boolean
}

/** A value the settings editor shows or sets. */
export type SettingValue = boolean | number | string | string[]

export interface SettingChoice {
  value: string
  label: string
}

/** What a setting holds, and how it's edited. */
export type SettingKind =
  | { type: 'boolean' }
  | { type: 'integer' | 'number'; minimum: number | null; maximum: number | null }
  /** `choices`: the values offered (built-in settings may hold others, set in the file). */
  | { type: 'string'; choices: SettingChoice[] | null }
  | { type: 'list' }
  /** Entries edited in `settings.toml`; the value is their names. */
  | { type: 'table' }

/** One setting, built in or declared by an extension (`settings_schema`). */
export type Setting = SettingKind & {
  /** Its keys in `settings.toml`, e.g. `["editor", "tab_size"]`. */
  path: string[]
  section: string
  /** The id of the extension that declares it. */
  extension: string | null
  title: string
  description: string
  default: SettingValue
  /** The value in effect. */
  value: SettingValue
  /** Whether `settings.toml` sets it. */
  modified: boolean
  /** Why the value in `settings.toml` isn't used. */
  problem: string | null
}

export interface SettingsSchema {
  /** The settings file, if there's a configuration directory. */
  file: string | null
  /** Why the settings file isn't in effect, if it's invalid. */
  error: string | null
  settings: Setting[]
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
  /** `announce`: tell the user (not when they changed one setting in the settings editor). */
  | { type: 'settingsChanged'; settings: FrontendSettings; announce: boolean }
  | { type: 'searchCounted'; documentId: number }
  | { type: 'syntaxUpdated'; documentId: number }
  | { type: 'diagnosticsChanged'; documentId: number }
  | { type: 'languageServerChanged' }
  | { type: 'languagesChanged' }
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
  | { type: 'extensionPreviewsChanged' }
  | { type: 'extensionPreviewChanged'; preview: string }
  | { type: 'extensionShowPreview'; preview: string; beside: boolean }
  | { type: 'notice'; level: 'warning'; message: string }

export const getWorkspace = () => call<WorkspaceInfo | null>('workspace_get')
export const openWorkspaceDialog = () => call<WorkspaceInfo | null>('workspace_open_dialog')
export const closeWorkspace = () => call<void>('workspace_close')
export const listDirectory = (path: string) =>
  call<DirectoryEntry[]>('workspace_list_dir', { path })
/** Creates an empty file or a folder; `name` may include subfolders (`src/lib.rs`). */
export const createEntry = (parent: string, name: string, directory: boolean) =>
  call<CreatedEntry>('workspace_create', { parent, name, directory })
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
export const setIconTheme = (iconTheme: string) =>
  call<void>('settings_set_icon_theme', { iconTheme })
export const getSettingsSchema = () => call<SettingsSchema>('settings_schema')
/** Sets one setting in `settings.toml`; `null` removes it, so its default applies. */
export const setSetting = (path: string[], value: SettingValue | null) =>
  call<void>('settings_set', { path, value })

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
