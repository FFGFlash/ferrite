import { formatUsage } from '../app/format'
import { executeCommand } from '../commands/registry'
import {
  restartExtension,
  setExtensionEnabled,
  setExtensionPermissions,
  type ExtensionState,
  type ExtensionStatus,
  type Permission,
  type ResourceUsage,
} from '../ipc/extensions'
import { extensionsStore, useExtensionUsage } from '../state/extensions'
import { reportError } from '../state/notifications'
import { useStore } from '../state/store'
import './ExtensionsPanel.css'

const stateLabels: Record<ExtensionState, string> = {
  inactive: 'Inactive',
  awaitingPermission: 'Needs permission',
  starting: 'Starting',
  active: 'Active',
  failed: 'Stopped',
  disabled: 'Disabled',
  invalid: 'Invalid',
  ready: 'Enabled',
}

/** What an extension adds to the workbench, e.g. "2 themes, 1 view". */
function contributionSummary(extension: ExtensionStatus): string | null {
  const { themes, keybindings, menus, views, decorations, languages, languageServers } =
    extension.contributions
  const parts = [
    [themes.length, 'theme'],
    [views.length, 'view'],
    [keybindings.length, 'keybinding'],
    [menus.length, 'menu item'],
    [decorations.length, 'decoration type'],
    [languageServers.length, 'language server'],
  ] as const
  const listed = [
    // Named, as a list of languages says more than a count.
    ...(languages.length > 0 ? [languages.map(language => language.name).join(', ')] : []),
    ...parts
      .filter(([count]) => count > 0)
      .map(([count, noun]) => `${count} ${noun}${count === 1 ? '' : 's'}`),
  ]
  return listed.length > 0 ? `Adds ${listed.join(', ')}` : null
}

/** Lists installed and development extensions with their state and permissions. */
export function ExtensionsPanel() {
  const extensions = useStore(extensionsStore)
  const usage = useExtensionUsage()
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
    <ul className="extensions-panel" aria-label="Extensions">
      {extensions.map(extension => (
        <ExtensionItem
          key={extension.id}
          extension={extension}
          usage={usage.get(extension.id) ?? extension.usage}
        />
      ))}
    </ul>
  )
}

function act(action: string, run: () => Promise<void>): void {
  run().catch((error: unknown) => reportError(action, error))
}

function ExtensionItem({ extension, usage }: { extension: ExtensionStatus; usage: ResourceUsage }) {
  const usageText = formatUsage(usage)
  const valid = extension.state !== 'invalid'
  const runs = valid && extension.state !== 'ready'
  const visibleCommands = extension.commands.filter(command => !command.hidden)
  const disabled = extension.state === 'disabled'
  const granted = extension.permissions.filter(p => p.granted).map(p => p.permission)
  const toggle = (permission: Permission, allow: boolean) => {
    const next = allow ? [...granted, permission] : granted.filter(p => p !== permission)
    act('Changing permissions', () => setExtensionPermissions(extension.id, next))
  }

  return (
    <li className="extension" data-state={extension.state}>
      <div className="extension__header">
        <span className="extension__name">{extension.name}</span>
        {extension.version && <span className="extension__version">{extension.version}</span>}
        {extension.development && <span className="extension__badge">dev</span>}
        <span className="extension__state">{stateLabels[extension.state]}</span>
      </div>
      {extension.description && <p className="extension__description">{extension.description}</p>}
      {contributionSummary(extension) && (
        <p className="extension__contributes">{contributionSummary(extension)}</p>
      )}
      {usageText && <p className="extension__usage">{usageText}</p>}
      {extension.error && (
        <p className="extension__error" role="alert">
          {extension.error}
        </p>
      )}
      {extension.configErrors.map(error => (
        <p key={error} className="extension__warning">
          Setting: {error}
        </p>
      ))}
      {extension.permissions.length > 0 && (
        <fieldset className="extension__permissions">
          <legend>Permissions</legend>
          {extension.permissions.map(permission => (
            <label key={permission.permission} className="extension__permission">
              <input
                type="checkbox"
                checked={permission.granted}
                onChange={event => toggle(permission.permission, event.currentTarget.checked)}
              />
              <span>
                {permission.description}
                {!permission.decided && <em> (not decided)</em>}
              </span>
            </label>
          ))}
        </fieldset>
      )}
      {visibleCommands.length > 0 && (
        <div className="extension__commands">
          {visibleCommands.map(command => (
            <button
              key={command.id}
              type="button"
              className="extension__command"
              disabled={disabled}
              title={`Run ${command.category}: ${command.title}`}
              onClick={() => void executeCommand(command.id)}
            >
              {command.title}
            </button>
          ))}
        </div>
      )}
      <div className="extension__footer">
        <span className="extension__path" title={extension.path}>
          {extension.path}
        </span>
        {valid && (
          <>
            {runs && (
              <button
                type="button"
                className="extensions-panel__button"
                disabled={disabled}
                onClick={() =>
                  act('Restarting the extension', () => restartExtension(extension.id))
                }
              >
                Restart
              </button>
            )}
            <button
              type="button"
              className="extensions-panel__button"
              onClick={() =>
                act(disabled ? 'Enabling the extension' : 'Disabling the extension', () =>
                  setExtensionEnabled(extension.id, disabled),
                )
              }
            >
              {disabled ? 'Enable' : 'Disable'}
            </button>
          </>
        )}
      </div>
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
