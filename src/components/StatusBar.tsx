import { memo } from "react";
import type { AppInfo } from "../ipc";
import type { LanguageStatus } from "../ipc/editor";
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
        {status?.language && <LanguageItems status={status.language} />}
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

function serverText(server: NonNullable<LanguageStatus["server"]>): string {
  switch (server.state) {
    case "starting":
      return `${server.name}: starting…`;
    case "busy":
      return `${server.name}: ${server.message ?? "working"}…`;
    case "failed":
      return `${server.name}: unavailable`;
    case "ready":
      return server.name;
  }
}

/** Problem counts, language, and language server for the active document. */
function LanguageItems({ status }: { status: LanguageStatus }) {
  const { server } = status;
  return (
    <>
      {server && server.state !== "failed" && (
        <span
          className="statusbar__item"
          title={`${status.errors} errors, ${status.warnings} warnings`}
          aria-label={`${status.errors} errors, ${status.warnings} warnings`}
        >
          ⊗ {status.errors} ⚠ {status.warnings}
        </span>
      )}
      <span className="statusbar__item" title="Language">
        {status.language}
      </span>
      {server && (
        <span
          className="statusbar__item"
          data-state={server.state}
          title={server.message ?? `Language server: ${server.name}`}
        >
          {serverText(server)}
        </span>
      )}
    </>
  );
}
