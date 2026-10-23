import { memo } from "react";
import { formatPlatform } from "../app/format";
import type { AppInfo } from "../ipc";
import "./StatusBar.css";

interface StatusBarProps {
  appInfo: AppInfo | null;
}

export const StatusBar = memo(function StatusBar({ appInfo }: StatusBarProps) {
  return (
    <footer className="statusbar">
      <div className="statusbar__group" />
      {appInfo && (
        <div className="statusbar__group">
          <span className="statusbar__item">
            {appInfo.name} {appInfo.version}
          </span>
          <span className="statusbar__item">{formatPlatform(appInfo)}</span>
        </div>
      )}
    </footer>
  );
});
