import { CircleAlert, Settings } from 'lucide-react'
import type { KeyboardEvent, MouseEvent } from 'react'
import { executeCommand } from '../commands/registry'
import { openContextMenu } from '../components/contextMenuState'
import type { ExtensionStatus } from '../ipc/extensions'
import { extensionsStore, openExtension } from '../state/extensions'
import { useStore } from '../state/store'
import { ExtensionIcon, duration, manageEntries, stateLabels, troubled } from './extensionInfo'
import './ExtensionsPanel.css'

/**
 * Installed and development extensions, one row each: icon, name,
 * description, author, and how long it took to start. Choosing one opens
 * its page (README, permissions, and the rest).
 */
export function ExtensionsPanel() {
  const extensions = useStore(extensionsStore)
  if (extensions.length === 0) {
    return (
      <div className="extensions-panel__empty">
        <p>No extensions are installed.</p>
        <p>
          Put extension folders in the <code>extensions</code> folder next to your settings file, or
          load one you are developing.
        </p>
        <button
          type="button"
          className="extensions-panel__button"
          onClick={() => void executeCommand('extensions.addDevelopment')}
        >
          Load Development Extension…
        </button>
      </div>
    )
  }
  return (
    <ul className="extensions-panel" aria-label="Extensions" onKeyDown={onListKeys}>
      {extensions.map(extension => (
        <ExtensionRow key={extension.id} extension={extension} />
      ))}
    </ul>
  )
}

/** Moves between rows with the arrow keys, Home, and End. */
function onListKeys(event: KeyboardEvent<HTMLElement>): void {
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
    return
  }
  const rows = [...event.currentTarget.querySelectorAll<HTMLElement>('.extension-row__open')]
  const index = rows.indexOf(document.activeElement as HTMLElement)
  if (index === -1) {
    return
  }
  event.preventDefault()
  const next =
    event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? rows.length - 1
        : Math.min(Math.max(index + (event.key === 'ArrowDown' ? 1 : -1), 0), rows.length - 1)
  rows[next]?.focus()
}

/** Everything the row shortens, for its tooltip. */
export function rowTooltip(extension: ExtensionStatus): string {
  const title = [extension.name, extension.version, extension.development ? '(development)' : '']
    .filter(Boolean)
    .join(' ')
  const by = extension.author ? `${extension.id} · by ${extension.author}` : extension.id
  const started =
    extension.usage.activationMs === null
      ? ''
      : ` · started in ${duration(extension.usage.activationMs)}`
  return [
    title,
    by,
    extension.description,
    `${stateLabels[extension.state]}${started}`,
    extension.error ?? '',
  ]
    .filter(Boolean)
    .join('\n')
}

function ExtensionRow({ extension }: { extension: ExtensionStatus }) {
  const { activationMs } = extension.usage
  const showMenu = (x: number, y: number) => openContextMenu(x, y, manageEntries(extension))
  const onContextMenu = (event: MouseEvent) => {
    event.preventDefault()
    showMenu(event.clientX, event.clientY)
  }
  const stateText =
    extension.state === 'active' || extension.state === 'ready'
      ? null
      : stateLabels[extension.state]
  return (
    <li className="extension-row" data-state={extension.state} onContextMenu={onContextMenu}>
      <button
        type="button"
        className="extension-row__open"
        title={rowTooltip(extension)}
        aria-label={`${extension.name}${stateText ? `, ${stateText}` : ''}`}
        aria-description={extension.description || undefined}
        onClick={() => openExtension(extension.id)}
      >
        <ExtensionIcon extension={extension} size={36} />
        <span className="extension-row__text">
          <span className="extension-row__line">
            <span className="extension-row__name">{extension.name}</span>
            {extension.development && <span className="extension-row__badge">dev</span>}
            {activationMs !== null && (
              <span className="extension-row__time" aria-hidden="true">
                {duration(activationMs)}
              </span>
            )}
          </span>
          <span className="extension-row__description">
            {extension.description || extension.error || ' '}
          </span>
          <span className="extension-row__line">
            <span className="extension-row__author">{extension.author ?? extension.id}</span>
            {stateText && (
              <span className="extension-row__state">
                {troubled(extension.state) && (
                  <CircleAlert size={12} strokeWidth={1.75} aria-hidden="true" />
                )}
                {stateText}
              </span>
            )}
          </span>
        </span>
      </button>
      <button
        type="button"
        className="extension-row__manage"
        title="Manage"
        aria-label={`Manage ${extension.name}`}
        aria-haspopup="menu"
        onClick={event => {
          const rect = event.currentTarget.getBoundingClientRect()
          showMenu(rect.left, rect.bottom)
        }}
      >
        <Settings size={14} strokeWidth={1.5} aria-hidden="true" />
      </button>
    </li>
  )
}

/** Buttons in the side bar header. */
export function ExtensionsActions() {
  return (
    <>
      <button
        type="button"
        className="dock-action"
        title="Load Development Extension…"
        aria-label="Load Development Extension"
        onClick={() => void executeCommand('extensions.addDevelopment')}
      >
        ⊕
      </button>
      <button
        type="button"
        className="dock-action"
        title="Reload Extensions"
        aria-label="Reload Extensions"
        onClick={() => void executeCommand('extensions.reload')}
      >
        ↻
      </button>
    </>
  )
}
