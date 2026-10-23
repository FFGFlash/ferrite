/** Applies backend notifications to frontend state. */
import { activeEditor } from "../editor/activeEditor";
import { subscribeToEvents, type AppEvent } from "../ipc/workspace";
import { applySettings } from "./appearance";
import { updateSummary } from "./documents";
import { notify } from "./notifications";
import { refreshDirectories, setIndexing, setWorkspace } from "./workspace";

export function handleEvent(event: AppEvent): void {
  switch (event.type) {
    case "workspaceChanged":
      setWorkspace(event.workspace);
      break;
    case "filesChanged":
      refreshDirectories(event.directories);
      break;
    case "documentReloaded": {
      updateSummary(event.document);
      const editor = activeEditor.get();
      if (editor?.documentId === event.document.id) {
        editor.refresh();
      }
      break;
    }
    case "documentChangedOnDisk":
      notify(
        "info",
        `${event.document.title} was changed by another program. Your unsaved changes were kept; saving will ask before overwriting.`,
      );
      break;
    case "settingsChanged":
      applySettings(event.settings);
      notify("info", "Settings applied.");
      break;
    case "indexStatus":
      setIndexing(event.indexing);
      break;
  }
}

export function listenToBackend(): void {
  subscribeToEvents(handleEvent);
}
