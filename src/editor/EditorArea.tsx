import { memo } from "react";
import type { AppInfo } from "../ipc";
import type { DocumentSummary } from "../ipc/editor";
import { activate, close, documentsStore } from "../state/documents";
import { useStore } from "../state/store";
import { EditorView } from "./EditorView";
import { WelcomePage } from "./WelcomePage";
import "./EditorArea.css";

interface EditorAreaProps {
  appInfo: AppInfo | null;
}

export const EditorArea = memo(function EditorArea({ appInfo }: EditorAreaProps) {
  const { documents, activeId } = useStore(documentsStore);
  const active = documents.find((document) => document.id === activeId);

  return (
    <main className="editor-area">
      {documents.length > 0 && (
        <div className="editor-tabs" role="tablist" aria-label="Open editors">
          {documents.map((document) => (
            <EditorTab key={document.id} document={document} active={document.id === activeId} />
          ))}
        </div>
      )}
      {active ? (
        <section
          className="editor-area__content"
          role="tabpanel"
          id={`editor-panel-${active.id}`}
          aria-labelledby={`editor-tab-${active.id}`}
        >
          <EditorView key={active.id} documentId={active.id} title={active.title} />
        </section>
      ) : (
        <section className="editor-area__content editor-area__content--scroll">
          <WelcomePage appInfo={appInfo} />
        </section>
      )}
    </main>
  );
});

interface EditorTabProps {
  document: DocumentSummary;
  active: boolean;
}

function EditorTab({ document, active }: EditorTabProps) {
  const label = document.dirty ? `${document.title}, unsaved` : document.title;
  return (
    <div
      className="editor-tab"
      role="tab"
      id={`editor-tab-${document.id}`}
      aria-selected={active}
      aria-controls={active ? `editor-panel-${document.id}` : undefined}
      aria-label={label}
      data-dirty={document.dirty || undefined}
      title={document.path ?? document.title}
      onMouseDown={(event) => {
        if (event.button === 0) {
          activate(document.id);
        }
      }}
      onAuxClick={(event) => {
        if (event.button === 1) {
          void close(document.id);
        }
      }}
    >
      <span className="editor-tab__title">{document.title}</span>
      <button
        type="button"
        className="editor-tab__close"
        aria-label={`Close ${document.title}`}
        onMouseDown={(event) => event.stopPropagation()}
        onClick={() => void close(document.id)}
      >
        <span className="editor-tab__dirty" aria-hidden="true" />
        <span className="editor-tab__x" aria-hidden="true">
          ×
        </span>
      </button>
    </div>
  );
}
