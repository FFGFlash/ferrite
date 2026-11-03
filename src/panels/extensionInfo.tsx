/**
 * What the Extensions view and an extension's page both show: its state,
 * what it adds, its icon, and the actions on it.
 */
import { Blocks } from 'lucide-react'
import type { MenuEntry } from '../components/contextMenuState'
import {
  restartExtension,
  setExtensionEnabled,
  type ExtensionState,
  type ExtensionStatus,
} from '../ipc/extensions'
import { writeClipboardText } from '../ipc/workspace'
import { openExtension, useExtensionIcon } from '../state/extensions'
import { reportError } from '../state/notifications'
import { extensionChannel, showOutput } from '../state/output'
import { extensionQuery, openSettings } from '../state/settings'

export const stateLabels: Record<ExtensionState, string> = {
  inactive: 'Inactive',
  awaitingPermission: 'Needs permission',
  starting: 'Starting',
  active: 'Active',
  failed: 'Stopped',
  disabled: 'Disabled',
  invalid: 'Invalid',
  ready: 'Enabled',
}

/** States that need the user's attention. */
export const troubled = (state: ExtensionState) =>
  state === 'failed' || state === 'invalid' || state === 'awaitingPermission'

/** What an extension adds to the workbench, e.g. ["Rust", "2 themes", "1 view"]. */
export function contributionList(extension: ExtensionStatus): string[] {
  const { themes, iconThemes, keybindings, menus, views, decorations, previews, languages } =
    extension.contributions
  const parts = [
    [themes.length, 'color theme'],
    [iconThemes.length, 'file icon theme'],
    [views.length, 'view'],
    [previews.length, 'preview'],
    [keybindings.length, 'keybinding'],
    [menus.length, 'menu item'],
    [decorations.length, 'decoration type'],
    [extension.contributions.languageServers.length, 'language server'],
    [extension.contributions.colors.length, 'color'],
  ] as const
  const { programs } = extension.contributions
  return [
    // Named, as a list of languages says more than a count.
    ...(languages.length > 0
      ? [
          `${languages.length === 1 ? 'Language' : 'Languages'}: ${languages.map(l => l.name).join(', ')}`,
        ]
      : []),
    // Named too: they run on the user's computer.
    ...(programs.length > 0
      ? [
          `${programs.length === 1 ? 'Program' : 'Programs'}: ${programs.map(p => p.command).join(', ')}`,
        ]
      : []),
    ...parts
      .filter(([count]) => count > 0)
      .map(([count, noun]) => `${count} ${noun}${count === 1 ? '' : 's'}`),
  ]
}

/** A duration in milliseconds, e.g. "12 ms" or "1.4 s". */
export function duration(ms: number): string {
  if (ms >= 1000) {
    return `${(ms / 1000).toFixed(1)} s`
  }
  return `${ms < 10 ? ms.toFixed(1) : Math.round(ms)} ms`
}

/** The extension's icon, or a plain one if it has none. */
export function ExtensionIcon({ extension, size }: { extension: ExtensionStatus; size: number }) {
  const icon = useExtensionIcon(extension)
  return icon ? (
    <img
      className="extension-icon"
      src={icon}
      alt=""
      width={size}
      height={size}
      draggable={false}
    />
  ) : (
    <span
      className="extension-icon extension-icon--none"
      style={{ width: size, height: size }}
      aria-hidden="true"
    >
      <Blocks size={Math.round(size * 0.55)} strokeWidth={1.25} />
    </span>
  )
}

function act(action: string, run: () => Promise<void>): void {
  run().catch((error: unknown) => reportError(action, error))
}

/** Whether the extension has code that can be restarted. */
export const canRestart = (extension: ExtensionStatus) =>
  extension.state !== 'invalid' && extension.state !== 'ready' && extension.state !== 'disabled'

export function toggleEnabled(extension: ExtensionStatus): void {
  const disabled = extension.state === 'disabled'
  act(disabled ? 'Enabling the extension' : 'Disabling the extension', () =>
    setExtensionEnabled(extension.id, disabled),
  )
}

export function restart(extension: ExtensionStatus): void {
  act('Restarting the extension', () => restartExtension(extension.id))
}

/** The extension's actions, for its manage menu. */
export function manageEntries(extension: ExtensionStatus, onPage = false): MenuEntry[] {
  if (extension.state === 'invalid') {
    return [
      ...(onPage ? [] : [{ label: 'Show Details', run: () => openExtension(extension.id) }]),
      { label: 'Copy Folder Path', run: () => copy(extension.path) },
    ]
  }
  const disabled = extension.state === 'disabled'
  return [
    ...(onPage ? [] : [{ label: 'Show Details', run: () => openExtension(extension.id) }, null]),
    { label: disabled ? 'Enable' : 'Disable', run: () => toggleEnabled(extension) },
    ...(canRestart(extension) ? [{ label: 'Restart', run: () => restart(extension) }] : []),
    null,
    ...(extension.hasSettings
      ? [{ label: 'Settings', run: () => openSettings(extensionQuery(extension.id)) }]
      : []),
    { label: 'Show Log', run: () => showOutput(extensionChannel(extension.id)) },
    null,
    { label: 'Copy Extension ID', run: () => copy(extension.id) },
  ]
}

function copy(text: string): void {
  writeClipboardText(text).catch((error: unknown) => reportError('Copying', error))
}
