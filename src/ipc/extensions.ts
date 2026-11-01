/**
 * Typed wrappers for the extension commands in `src-tauri/src/extensions.rs`.
 */
import { call } from './index'

export type Permission = 'documents-read' | 'documents-write' | 'language-servers'

export type ExtensionState =
  | 'inactive'
  | 'awaitingPermission'
  | 'starting'
  | 'active'
  | 'failed'
  | 'disabled'
  | 'invalid'
  | 'ready'

export interface PermissionStatus {
  permission: Permission
  description: string
  granted: boolean
  /** Whether the user has granted or denied it. */
  decided: boolean
}

export interface ExtensionCommand {
  id: string
  title: string
  category: string
  /** Not listed in the command palette. */
  hidden: boolean
}

export interface ThemeContribution {
  /** `<extension id>.<theme id>`. */
  id: string
  label: string
  kind: 'dark' | 'light'
  colors: Record<string, string>
}

export interface IconThemeContribution {
  /** `<extension id>.<icon theme id>`. */
  id: string
  label: string
}

/** Icon ids by what they are for. Names and extensions are lowercase. */
export interface IconAssociations {
  file: string | null
  folder: string | null
  folderExpanded: string | null
  /** Without the leading dot, e.g. `rs` or `d.ts`. */
  fileExtensions: Record<string, string>
  fileNames: Record<string, string>
  folderNames: Record<string, string>
  folderNamesExpanded: Record<string, string>
}

/** A file icon theme (`extensions_icon_theme`). */
export interface IconTheme {
  /** Icon id to a `data:` URL. */
  icons: Record<string, string>
  associations: IconAssociations
  /** Looked up before `associations` with a light color theme. */
  light: IconAssociations
}

export interface KeybindingContribution {
  command: string
  key: string
  mac: string | null
  editorFocus: boolean
}

export type MenuLocation = 'editor/context' | 'explorer/context'

export interface MenuContribution {
  location: MenuLocation
  command: string
  group: string
}

export interface ViewContribution {
  id: string
  /** `<extension id>.<view id>`. */
  fullId: string
  title: string
  icon: string
}

export interface DecorationColors {
  background: string | null
  color: string | null
  border: string | null
  underlineColor: string | null
}

export interface DecorationType {
  name: string
  /** `<extension id>/<name>`, as in `DecorationView.key`. */
  key: string
  dark: DecorationColors
  light: DecorationColors
  underline: 'solid' | 'wavy' | 'dotted' | 'dashed' | null
  bold: boolean
  italic: boolean
}

/** A language an extension adds, highlighted by its own grammar. */
export interface LanguageContribution {
  /** The language id, e.g. `html`. */
  id: string
  name: string
  fileExtensions: string[]
  fileNames: string[]
}

/** A language server an extension declares; run once allowed. */
export interface LanguageServerContribution {
  name: string
  command: string
  args: string[]
  languages: string[]
  rootMarkers: string[]
}

export interface Contributions {
  themes: ThemeContribution[]
  iconThemes: IconThemeContribution[]
  keybindings: KeybindingContribution[]
  menus: MenuContribution[]
  views: ViewContribution[]
  decorations: DecorationType[]
  languages: LanguageContribution[]
  languageServers: LanguageServerContribution[]
}

export interface StatusItem {
  /** `<extension id>.<item id>`. */
  id: string
  /** The extension's name. */
  extension: string
  alignment: 'left' | 'right'
  priority: number
  text: string
  tooltip: string | null
  command: string | null
}

export interface ViewEntry {
  id: string
  parent: string | null
  label: string
  description: string | null
  tooltip: string | null
  expanded: boolean
  command: string | null
  argument: string | null
}

export interface ViewContent {
  message: string | null
  items: ViewEntry[]
}

export interface ExtensionStatus {
  id: string
  name: string
  version: string
  description: string
  /** The extension's folder. */
  path: string
  development: boolean
  state: ExtensionState
  error: string | null
  permissions: PermissionStatus[]
  commands: ExtensionCommand[]
  configErrors: string[]
  contributions: Contributions
  usage: ResourceUsage
}

/** What an extension has used during this session. */
export interface ResourceUsage {
  /** Time spent running its code. */
  busyMs: number
  calls: number
  slowestCallMs: number
  /** Memory of the running instance; 0 when not running. */
  memoryBytes: number
  peakMemoryBytes: number
  /** Document changes dropped because it was busy. */
  droppedEvents: number
  compileMs: number
}

export type MessageLevel = 'info' | 'warning' | 'error'

export const listExtensions = () => call<ExtensionStatus[]>('extensions_list')
export const reloadExtensions = () => call<void>('extensions_reload')
/** Runs an extension command for the active document; resolves when it finishes. */
export const runExtensionCommand = (
  command: string,
  documentId: number | null,
  argument: string | null = null,
) => call<void>('extension_run_command', { command, documentId, argument })
/** A file icon theme with its icons; `null` if no enabled extension has it. */
export const getIconTheme = (id: string) => call<IconTheme | null>('extensions_icon_theme', { id })
export const listStatusItems = () => call<StatusItem[]>('extension_status_items')
export const getViewContent = (view: string) => call<ViewContent>('extension_view', { view })
/** Tells the backend a view is shown, which may start its extension. */
export const viewOpened = (view: string) => call<void>('extension_view_opened', { view })
/** Grants `granted` and denies the extension's other requested permissions. */
export const setExtensionPermissions = (id: string, granted: Permission[]) =>
  call<void>('extension_set_permissions', { id, granted })
export const setExtensionEnabled = (id: string, enabled: boolean) =>
  call<void>('extension_set_enabled', { id, enabled })
export const restartExtension = (id: string) => call<void>('extension_restart', { id })
/** Asks for a folder and loads it as a development extension. */
export const addDevelopmentExtension = () => call<string | null>('extension_add_development')
