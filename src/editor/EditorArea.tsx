import { memo } from "react";
import type { AppInfo } from "../ipc";
import { WelcomePage } from "./WelcomePage";
import "./EditorArea.css";

interface EditorAreaProps {
  appInfo: AppInfo | null;
}

export const EditorArea = memo(function EditorArea({ appInfo }: EditorAreaProps) {
  return (
    <main className="editor-area">
      <div className="editor-tabs" role="tablist" aria-label="Open editors">
        <div
          className="editor-tab"
          role="tab"
          id="editor-tab-welcome"
          aria-selected="true"
          aria-controls="editor-panel-welcome"
        >
          Welcome
        </div>
      </div>
      <section
        className="editor-area__content"
        role="tabpanel"
        id="editor-panel-welcome"
        aria-labelledby="editor-tab-welcome"
      >
        <WelcomePage appInfo={appInfo} />
      </section>
    </main>
  );
});
