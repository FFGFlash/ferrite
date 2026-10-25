import { formatPlatform } from '../app/format'
import { executeCommand, getCommand, primaryKeybinding } from '../commands/registry'
import type { AppInfo } from '../ipc'
import './WelcomePage.css'

interface WelcomePageProps {
  appInfo: AppInfo | null
}

const actions: { command: string; label: string }[] = [
  { command: 'file.newFile', label: 'New File' },
  { command: 'file.openFile', label: 'Open File…' },
  { command: 'file.openFolder', label: 'Open Folder…' },
  { command: 'workbench.commandPalette', label: 'Show All Commands' },
]

export function WelcomePage({ appInfo }: WelcomePageProps) {
  return (
    <div className="welcome">
      <h1 className="welcome__title">Ferrite</h1>
      <p className="welcome__tagline">A fast, extensible code editor.</p>
      <ul className="welcome__actions">
        {actions.map(({ command, label }) => {
          const shortcut = primaryKeybinding(getCommand(command))
          return (
            <li key={command}>
              <button
                type="button"
                className="welcome__action"
                onClick={() => void executeCommand(command)}
              >
                <span>{label}</span>
                {shortcut && <kbd>{shortcut}</kbd>}
              </button>
            </li>
          )
        })}
      </ul>
      {appInfo && (
        <dl className="welcome__facts">
          <dt>Version</dt>
          <dd>{appInfo.version}</dd>
          <dt>Platform</dt>
          <dd>{formatPlatform(appInfo)}</dd>
          <dt>Tauri</dt>
          <dd>{appInfo.tauriVersion}</dd>
        </dl>
      )}
    </div>
  )
}
