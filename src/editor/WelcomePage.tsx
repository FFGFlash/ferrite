import { formatPlatform } from "../app/format";
import type { AppInfo } from "../ipc";
import "./WelcomePage.css";

interface WelcomePageProps {
  appInfo: AppInfo | null;
}

export function WelcomePage({ appInfo }: WelcomePageProps) {
  return (
    <div className="welcome">
      <h1 className="welcome__title">Ferrite</h1>
      <p className="welcome__tagline">A fast, extensible code editor.</p>
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
