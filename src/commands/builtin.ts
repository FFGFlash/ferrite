/** The workbench's built-in commands and their default keybindings. */
import { openPalette } from '../components/paletteState'
import { togglePerfOverlay } from '../components/PerfOverlay'
import { resetPerf } from '../state/perf'
import { activeEditor } from '../editor/activeEditor'
import { close, documentsStore, newFile, openFile, openPath, save } from '../state/documents'
import { addDevelopmentExtension, reloadExtensions } from '../ipc/extensions'
import { setIconTheme, setTheme, settingsFilePath } from '../ipc/workspace'
import { DEFAULT_ICON_THEME } from '../state/iconTheme'
import { layoutStore, updateLayout } from '../state/layout'
import { notify } from '../state/notifications'
import { closeFolder, collapseAll, openFolder, workspaceStore } from '../state/workspace'
import { isMac } from '../editor/keymap'
import { beginCreate } from '../panels/explorerState'
import { clearTerminal, closeTerminal, terminalsStore } from '../terminal/terminals'
import {
  closeCurrentTab,
  cycleTab,
  focusActiveTerminal,
  focusGroupRelative,
  hasCurrentTab,
  moveCurrentTab,
  openTerminal,
  resetLayout,
  showView,
  splitCurrentTab,
  toggleTerminal,
  toggleTerminals,
  toggleViews,
} from '../dock/dock'
import { bindingsOf, commandLabel, formatKeybinding, listCommands, type Command } from './registry'

const hasEditor = () => activeEditor.get() !== null
const hasDocument = () => documentsStore.get().activeId !== null
const hasWorkspace = () => workspaceStore.get().info !== null
const hasTerminal = () => terminalsStore.get().activeId !== null
const editor = () => activeEditor.get()

export const builtinCommands: Command[] = [
  { id: 'file.newFile', category: 'File', title: 'New File', keybinding: 'mod+n', run: newFile },
  {
    id: 'file.openFile',
    category: 'File',
    title: 'Open File…',
    keybinding: 'mod+o',
    run: openFile,
  },
  { id: 'file.openFolder', category: 'File', title: 'Open Folder…', run: openFolder },
  {
    id: 'file.closeFolder',
    category: 'File',
    title: 'Close Folder',
    enabled: hasWorkspace,
    run: closeFolder,
  },
  {
    id: 'file.save',
    category: 'File',
    title: 'Save',
    keybinding: 'mod+s',
    enabled: hasDocument,
    run: () => save(),
  },
  {
    id: 'file.saveAs',
    category: 'File',
    title: 'Save As…',
    keybinding: 'mod+shift+s',
    enabled: hasDocument,
    run: () => save(undefined, true),
  },
  {
    id: 'file.close',
    category: 'View',
    title: 'Close Editor',
    keybinding: 'mod+w',
    enabled: hasDocument,
    run: () => close(),
  },
  {
    id: 'workbench.commandPalette',
    category: 'View',
    title: 'Command Palette',
    keybinding: ['mod+shift+p', 'f1'],
    terminal: true,
    hidden: true,
    run: () => openPalette('>'),
  },
  {
    id: 'workbench.quickOpen',
    category: 'Go',
    title: 'Go to File…',
    keybinding: 'mod+p',
    terminal: true,
    run: () => openPalette(''),
  },
  {
    id: 'workbench.goToLine',
    category: 'Go',
    title: 'Go to Line…',
    keybinding: 'ctrl+g',
    enabled: hasEditor,
    run: () => openPalette(':'),
  },
  {
    id: 'workbench.toggleSidebar',
    category: 'View',
    title: 'Toggle Side Bar',
    keybinding: 'mod+b',
    terminal: true,
    run: toggleViews,
  },
  {
    id: 'view.toggleMinimap',
    category: 'View',
    title: 'Toggle Minimap',
    run: () => updateLayout({ minimapVisible: !layoutStore.get().minimapVisible }),
  },
  {
    id: 'workbench.togglePanel',
    category: 'View',
    title: 'Toggle Terminals',
    keybinding: 'mod+j',
    terminal: true,
    run: toggleTerminals,
  },
  {
    id: 'terminal.toggle',
    category: 'View',
    title: 'Toggle Terminal',
    keybinding: 'ctrl+`',
    terminal: true,
    run: toggleTerminal,
  },
  {
    id: 'terminal.new',
    category: 'Terminal',
    title: 'New Terminal',
    keybinding: 'ctrl+shift+`',
    terminal: true,
    run: openTerminal,
  },
  {
    id: 'terminal.focus',
    category: 'Terminal',
    title: 'Focus Terminal',
    run: focusActiveTerminal,
  },
  {
    id: 'terminal.kill',
    category: 'Terminal',
    title: 'Kill Terminal',
    enabled: hasTerminal,
    run: () => closeTerminal(),
  },
  {
    id: 'terminal.clear',
    category: 'Terminal',
    title: 'Clear',
    enabled: hasTerminal,
    run: () => clearTerminal(),
  },
  {
    id: 'view.closeTab',
    category: 'View',
    title: 'Close Tab',
    enabled: hasCurrentTab,
    run: closeCurrentTab,
  },
  ...(['left', 'right', 'up', 'down'] as const).map((direction): Command => ({
    id: `view.moveTab${direction[0]!.toUpperCase()}${direction.slice(1)}`,
    category: 'View',
    title: `Move Tab ${direction[0]!.toUpperCase()}${direction.slice(1)}`,
    enabled: hasCurrentTab,
    run: () =>
      moveCurrentTab(direction === 'up' ? 'top' : direction === 'down' ? 'bottom' : direction),
  })),
  {
    id: 'view.splitRight',
    category: 'View',
    title: 'Split Tab Right',
    enabled: hasCurrentTab,
    run: () => splitCurrentTab('right'),
  },
  {
    id: 'view.splitDown',
    category: 'View',
    title: 'Split Tab Down',
    enabled: hasCurrentTab,
    run: () => splitCurrentTab('bottom'),
  },
  {
    id: 'view.focusNextGroup',
    category: 'View',
    title: 'Focus Next Group',
    keybinding: 'f6',
    terminal: true,
    run: () => focusGroupRelative(1),
  },
  {
    id: 'view.focusPreviousGroup',
    category: 'View',
    title: 'Focus Previous Group',
    keybinding: 'shift+f6',
    terminal: true,
    run: () => focusGroupRelative(-1),
  },
  {
    id: 'view.resetLayout',
    category: 'View',
    title: 'Reset Layout',
    run: resetLayout,
  },
  {
    id: 'workbench.showExplorer',
    category: 'View',
    title: 'Show Explorer',
    keybinding: 'mod+shift+e',
    terminal: true,
    run: () => showView('explorer'),
  },
  {
    id: 'workbench.showSearch',
    category: 'View',
    title: 'Search in Files',
    keybinding: 'mod+shift+f',
    terminal: true,
    run: () => showView('search'),
  },
  {
    id: 'workbench.showExtensions',
    category: 'View',
    title: 'Extensions',
    keybinding: 'mod+shift+x',
    terminal: true,
    run: () => showView('extensions'),
  },
  {
    id: 'extensions.reload',
    category: 'Extensions',
    title: 'Reload Extensions',
    run: async () => {
      await reloadExtensions()
      notify('info', 'Extensions reloaded.')
    },
  },
  {
    id: 'extensions.addDevelopment',
    category: 'Extensions',
    title: 'Load Development Extension…',
    run: async () => {
      if (await addDevelopmentExtension()) {
        showView('extensions')
      }
    },
  },
  {
    id: 'explorer.newFile',
    category: 'Explorer',
    title: 'New File…',
    enabled: hasWorkspace,
    run: () => {
      showView('explorer', false)
      beginCreate(false)
    },
  },
  {
    id: 'explorer.newFolder',
    category: 'Explorer',
    title: 'New Folder…',
    enabled: hasWorkspace,
    run: () => {
      showView('explorer', false)
      beginCreate(true)
    },
  },
  {
    id: 'explorer.collapseAll',
    category: 'Explorer',
    title: 'Collapse Folders',
    enabled: hasWorkspace,
    run: collapseAll,
  },
  {
    id: 'view.nextEditor',
    category: 'View',
    title: 'Next Tab',
    keybinding: ['ctrl+tab', 'ctrl+pagedown'],
    terminal: true,
    enabled: hasCurrentTab,
    run: () => cycleTab(1),
  },
  {
    id: 'view.previousEditor',
    category: 'View',
    title: 'Previous Tab',
    keybinding: ['ctrl+shift+tab', 'ctrl+pageup'],
    terminal: true,
    enabled: hasCurrentTab,
    run: () => cycleTab(-1),
  },
  {
    id: 'editor.find',
    category: 'Edit',
    title: 'Find',
    keybinding: 'mod+f',
    enabled: hasEditor,
    run: () => editor()?.openFind(false),
  },
  {
    id: 'editor.replace',
    category: 'Edit',
    title: 'Replace',
    keybinding: { default: 'mod+h', mac: 'mod+alt+f' },
    enabled: hasEditor,
    run: () => editor()?.openFind(true),
  },
  {
    id: 'editor.findNext',
    category: 'Edit',
    title: 'Find Next',
    keybinding: 'f3',
    enabled: hasEditor,
    run: () => editor()?.findNext(false),
  },
  {
    id: 'editor.findPrevious',
    category: 'Edit',
    title: 'Find Previous',
    keybinding: 'shift+f3',
    enabled: hasEditor,
    run: () => editor()?.findNext(true),
  },
  {
    id: 'editor.addNextMatch',
    category: 'Selection',
    title: 'Add Next Occurrence',
    keybinding: 'mod+d',
    editorFocus: true,
    enabled: hasEditor,
    run: () => editor()?.dispatch({ type: 'addNextMatch' }),
  },
  {
    id: 'editor.goToDefinition',
    category: 'Go',
    title: 'Go to Definition',
    keybinding: 'f12',
    enabled: hasEditor,
    run: () => editor()?.goToDefinition(),
  },
  {
    id: 'editor.triggerSuggest',
    category: 'Edit',
    title: 'Trigger Suggest',
    keybinding: 'ctrl+space',
    editorFocus: true,
    enabled: hasEditor,
    run: () => editor()?.triggerSuggest(),
  },
  {
    id: 'editor.showHover',
    category: 'Edit',
    title: 'Show Hover',
    enabled: hasEditor,
    run: () => editor()?.showHover(),
  },
  {
    id: 'editor.selectAll',
    category: 'Selection',
    title: 'Select All',
    enabled: hasEditor,
    run: () => editor()?.dispatch({ type: 'selectAll' }),
  },
  {
    id: 'editor.undo',
    category: 'Edit',
    title: 'Undo',
    enabled: hasEditor,
    run: () => editor()?.dispatch({ type: 'undo' }),
  },
  {
    id: 'editor.redo',
    category: 'Edit',
    title: 'Redo',
    enabled: hasEditor,
    run: () => editor()?.dispatch({ type: 'redo' }),
  },
  {
    id: 'preferences.openSettings',
    category: 'Preferences',
    title: 'Open Settings (TOML)',
    keybinding: 'mod+,',
    terminal: true,
    run: async () => openPath(await settingsFilePath()),
  },
  {
    id: 'developer.togglePerformance',
    category: 'Developer',
    title: 'Toggle Performance Overlay',
    run: togglePerfOverlay,
  },
  {
    id: 'developer.resetPerformance',
    category: 'Developer',
    title: 'Reset Frontend Performance Counters',
    run: resetPerf,
  },
  {
    id: 'developer.reloadWindow',
    category: 'Developer',
    title: 'Reload Window',
    // Documents, unsaved changes, and extensions live in the backend, so
    // the reloaded workbench picks up where it was.
    run: () => window.location.reload(),
  },
  {
    id: 'preferences.themeDark',
    category: 'Preferences',
    title: 'Color Theme: Dark',
    run: () => setTheme('dark'),
  },
  {
    id: 'preferences.themeLight',
    category: 'Preferences',
    title: 'Color Theme: Light',
    run: () => setTheme('light'),
  },
  {
    id: 'preferences.themeSystem',
    category: 'Preferences',
    title: 'Color Theme: Follow System',
    run: () => setTheme('system'),
  },
  {
    id: 'preferences.iconThemeDefault',
    category: 'Preferences',
    title: 'File Icon Theme: Default',
    run: () => setIconTheme(DEFAULT_ICON_THEME),
  },
  {
    id: 'preferences.keyboardShortcuts',
    category: 'Preferences',
    title: 'Keyboard Shortcuts',
    run: () => newFile(keyboardShortcutsText()),
  },
]

/**
 * A plain-text list of every command with its id and keys, shown in a new
 * document so it can be searched, for writing `[keybindings]` settings.
 */
export function keyboardShortcutsText(mac = isMac()): string {
  const rows = listCommands()
    .filter(command => !command.hidden)
    .map(command => [
      commandLabel(command),
      command.id,
      bindingsOf(command, mac)
        .map(binding => formatKeybinding(binding, mac))
        .join(', '),
    ])
    .sort((a, b) => a[0]!.localeCompare(b[0]!))
  const header = ['Command', 'Id', 'Keys']
  const widths = [0, 1].map(column =>
    Math.max(header[column]!.length, ...rows.map(row => row[column]!.length)),
  )
  const line = (row: string[]) =>
    `${row[0]!.padEnd(widths[0]!)}  ${row[1]!.padEnd(widths[1]!)}  ${row[2]}`.trimEnd()
  return [
    'Keyboard shortcuts',
    '',
    'Change them in settings.toml (Preferences: Open Settings) under [keybindings], e.g.',
    '  "workbench.showSearch" = "mod+shift+h"     "mod" is Ctrl, or Cmd on macOS; "" removes them',
    '',
    line(header),
    ...rows.map(line),
    '',
  ].join('\n')
}
