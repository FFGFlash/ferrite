import { formatPlatform } from "../app/format";
import { runAction, shortcutLabel, type WorkbenchAction } from "../app/shortcuts";
import type { AppInfo } from "../ipc";
import { isMac } from "./keymap";
import "./WelcomePage.css";

interface WelcomePageProps {
  appInfo: AppInfo | null;
}

const actions: { action: WorkbenchAction; label: string }[] = [
  { action: "newFile", label: "New File" },
  { action: "openFile", label: "Open File…" },
];

export function WelcomePage({ appInfo }: WelcomePageProps) {
  const mac = isMac();
  return (
    <div className="welcome">
      <h1 className="welcome__title">Ferrite</h1>
      <p className="welcome__tagline">A fast, extensible code editor.</p>
      <ul className="welcome__actions">
        {actions.map(({ action, label }) => (
          <li key={action}>
            <button type="button" className="welcome__action" onClick={() => runAction(action)}>
              <span>{label}</span>
              <kbd>{shortcutLabel(action, mac)}</kbd>
            </button>
          </li>
        ))}
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
  );
}
