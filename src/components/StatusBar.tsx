import { memo } from "react";
import type { AppInfo } from "../ipc";
import { documentsStore } from "../state/documents";
import { editorStatus } from "../state/editorStatus";
import { useStore } from "../state/store";
import "./StatusBar.css";

interface StatusBarProps {
  appInfo: AppInfo | null;
}

export const StatusBar = memo(function StatusBar({ appInfo }: StatusBarProps) {
  const status = useStore(editorStatus);
  const { documents, activeId } = useStore(documentsStore);
  const active = documents.find((document) => document.id === activeId);

  return (
    <footer className="statusbar">
      <div className="statusbar__group">
        {status && (
          <span className="statusbar__item">
            Ln {status.line}, Col {status.column}
            {status.selectionCount > 1 && ` (${status.selectionCount} selections)`}
          </span>
        )}
      </div>
      <div className="statusbar__group">
        {active && (
          <>
            <span className="statusbar__item" title="Line ending">
              {active.lineEnding}
            </span>
            <span className="statusbar__item" title="Encoding">
              {active.encoding}
            </span>
          </>
        )}
        {appInfo && (
          <span className="statusbar__item">
            {appInfo.name} {appInfo.version}
          </span>
        )}
      </div>
    </footer>
  );
});
