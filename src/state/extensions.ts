/**
 * Extensions as the workbench sees them: their status (for the Extensions
 * view), their contributions (commands with keybindings, themes, icon
 * themes, menu items, views, decoration types, status bar items), permission prompts,
 * and their messages and failures.
 */
import { useEffect, useState } from 'react'
import { registerCommands, type Command } from '../commands/registry'
import { activeEditor } from '../editor/activeEditor'
import {
  listExtensions,
  listStatusItems,
  restartExtension,
  runExtensionCommand,
  setExtensionPermissions,
  type DecorationType,
  type ExtensionStatus,
  type MenuLocation,
  type MessageLevel,
  type ResourceUsage,
  type StatusItem,
} from '../ipc/extensions'
import { setIconTheme, setTheme } from '../ipc/workspace'
import { extensionChannel, showOutput } from './output'
import { recordProblem } from './problems'
import { showView } from './viewRequests'
import { notify, reportError } from './notifications'
import { createStore } from './store'

export const extensionsStore = createStore<readonly ExtensionStatus[]>([])
/** Status bar items of extensions, as the backend last reported them. */
export const statusItemsStore = createStore<readonly StatusItem[]>([])
/** Bumped per view when its content changes, so the view reloads it. */
export const viewVersions = createStore<Readonly<Record<string, number>>>({})

let unregisterCommands: (() => void) | null = null
/** Extensions already asked about permissions this session. */
const prompted = new Set<string>()
let refreshing: Promise<void> | null = null
let refreshAgain = false

/** Extensions whose contributions apply: valid and not turned off. */
export function usableExtensions(statuses: readonly ExtensionStatus[]): ExtensionStatus[] {
  return statuses.filter(
    extension => extension.state !== 'disabled' && extension.state !== 'invalid',
  )
}

/**
 * Each extension's resource use by id, polled every `intervalMs` while the
 * calling component is mounted (usage changes without events).
 */
export function useExtensionUsage(intervalMs = 2000): ReadonlyMap<string, ResourceUsage> {
  const [usage, setUsage] = useState<ReadonlyMap<string, ResourceUsage>>(
    () => new Map(extensionsStore.get().map(extension => [extension.id, extension.usage])),
  )
  useEffect(() => {
    let cancelled = false
    const poll = () => {
      listExtensions()
        .then(statuses => {
          if (!cancelled) {
            setUsage(new Map(statuses.map(extension => [extension.id, extension.usage])))
          }
        })
        .catch(() => {})
    }
    const timer = setInterval(poll, intervalMs)
    return () => {
      cancelled = true
      clearInterval(timer)
    }
  }, [intervalMs])
  return usage
}

export function showExtensionsView(): void {
  showView('extensions', true)
}

/** Reloads extension statuses; coalesces calls made while one is running. */
export function refreshExtensions(): Promise<void> {
  if (refreshing) {
    refreshAgain = true
    return refreshing
  }
  refreshing = (async () => {
    do {
      refreshAgain = false
      try {
        applyStatuses(await listExtensions())
        statusItemsStore.set(await listStatusItems())
      } catch (error) {
        reportError('Loading extensions', error)
      }
    } while (refreshAgain)
    refreshing = null
  })()
  return refreshing
}

export async function refreshStatusItems(): Promise<void> {
  try {
    statusItemsStore.set(await listStatusItems())
  } catch (error) {
    reportError('Loading the status bar', error)
  }
}

export function viewChanged(view: string): void {
  viewVersions.update(versions => ({ ...versions, [view]: (versions[view] ?? 0) + 1 }))
}

export function applyStatuses(statuses: readonly ExtensionStatus[]): void {
  extensionsStore.set(statuses)
  registerContributedCommands(statuses)
  promptForPermissions(statuses)
}

/** Runs an extension command for the active document, with an argument. */
export function runContributed(command: string, argument: string | null = null): Promise<void> {
  return runExtensionCommand(command, activeEditor.get()?.documentId ?? null, argument)
}

function registerContributedCommands(statuses: readonly ExtensionStatus[]): void {
  unregisterCommands?.()
  const usable = usableExtensions(statuses)
  const commands: Command[] = usable.flatMap(extension =>
    extension.commands.map(command => {
      const bindings = extension.contributions.keybindings.filter(b => b.command === command.id)
      return {
        id: command.id,
        title: command.title,
        category: command.category,
        hidden: command.hidden,
        source: 'extension' as const,
        keybinding: bindings.map(binding => ({
          default: binding.key,
          mac: binding.mac ?? binding.key,
        })),
        editorFocus: bindings.some(binding => binding.editorFocus),
        run: () => runContributed(command.id),
      }
    }),
  )
  const themes: Command[] = usable.flatMap(extension =>
    extension.contributions.themes.map(theme => ({
      id: `preferences.theme.${theme.id}`,
      category: 'Preferences',
      title: `Color Theme: ${theme.label}`,
      source: 'extension' as const,
      run: () => setTheme(theme.id),
    })),
  )
  const iconThemes: Command[] = usable.flatMap(extension =>
    extension.contributions.iconThemes.map(theme => ({
      id: `preferences.iconTheme.${theme.id}`,
      category: 'Preferences',
      title: `File Icon Theme: ${theme.label}`,
      source: 'extension' as const,
      run: () => setIconTheme(theme.id),
    })),
  )
  unregisterCommands = registerCommands([...commands, ...themes, ...iconThemes])
}

/** A menu item contributed for `location`. */
export interface ContributedMenuItem {
  command: string
  title: string
  group: string
}

/** Contributed menu items for `location`, grouped and sorted. */
export function contributedMenuItems(location: MenuLocation): ContributedMenuItem[] {
  const items: ContributedMenuItem[] = []
  for (const extension of usableExtensions(extensionsStore.get())) {
    for (const menu of extension.contributions.menus) {
      const command = extension.commands.find(c => c.id === menu.command)
      if (menu.location === location && command) {
        items.push({ command: menu.command, title: command.title, group: menu.group })
      }
    }
  }
  return items.sort((a, b) => a.group.localeCompare(b.group) || a.title.localeCompare(b.title))
}

/** Decoration types by key, from usable extensions. */
export function decorationTypes(
  statuses: readonly ExtensionStatus[],
): ReadonlyMap<string, DecorationType> {
  return new Map(
    usableExtensions(statuses).flatMap(extension =>
      extension.contributions.decorations.map(decoration => [decoration.key, decoration] as const),
    ),
  )
}

function promptForPermissions(statuses: readonly ExtensionStatus[]): void {
  for (const extension of statuses) {
    if (extension.state !== 'awaitingPermission' || prompted.has(extension.id)) {
      continue
    }
    prompted.add(extension.id)
    const requested = extension.permissions.map(p => p.permission)
    const wants = extension.permissions.map(p => p.description.toLowerCase()).join('; ')
    // Only this asks, so it stays until answered.
    notify(
      'info',
      `The extension ${extension.name} wants to: ${wants}.`,
      [
        { label: 'Allow', run: () => setExtensionPermissions(extension.id, requested) },
        { label: "Don't Allow", run: () => setExtensionPermissions(extension.id, []) },
        { label: 'Details', run: showExtensionsView },
      ],
      { sticky: true },
    )
  }
}

/** A message an extension (`id`, named `name`) showed; warnings and errors are kept in Problems. */
export function showExtensionMessage(
  id: string,
  name: string,
  level: MessageLevel,
  message: string,
): void {
  notify(level, `${name}: ${message}`, undefined, { record: false })
  if (level !== 'info') {
    recordProblem(level, message, name, id)
  }
}

export function showExtensionFailure(id: string, name: string, reason: string): void {
  // Problems lists it while it's stopped, from its status.
  notify(
    'error',
    `The extension ${name} stopped: ${reason}`,
    [
      { label: 'Restart', run: () => restartExtension(id) },
      { label: 'Show Log', run: () => showOutput(extensionChannel(id)) },
    ],
    { record: false },
  )
}
