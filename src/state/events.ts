/** Applies backend notifications to frontend state. */
import { activeEditor } from "../editor/activeEditor";
import { subscribeToEvents, type AppEvent } from "../ipc/workspace";
import { applySettings } from "./appearance";
import { documentsStore, setDirty, updateSummary } from "./documents";
import { refreshExtensions, showExtensionFailure, showExtensionMessage } from "./extensions";
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
    case "languageServerChanged":
      activeEditor.get()?.refresh();
      break;
    case "searchCounted":
    case "syntaxUpdated":
    case "diagnosticsChanged": {
      const editor = activeEditor.get();
      if (editor?.documentId === event.documentId) {
        editor.refresh();
      }
      break;
    }
    case "documentOpened":
      documentsStore.update(({ documents, activeId }) => ({
        documents: documents.some((d) => d.id === event.document.id)
          ? documents
          : [...documents, event.document],
        activeId: event.active || activeId === null ? event.document.id : activeId,
      }));
      break;
    case "indexStatus":
      setIndexing(event.indexing);
      break;
    case "extensionsChanged":
      void refreshExtensions();
      break;
    case "extensionMessage":
      showExtensionMessage(event.extension, event.level, event.message);
      break;
    case "extensionFailed":
      showExtensionFailure(event.id, event.name, event.reason);
      break;
    case "documentEdited": {
      setDirty(event.documentId, true);
      const editor = activeEditor.get();
      if (editor?.documentId === event.documentId) {
        editor.refresh();
      }
      break;
    }
  }
}

export function listenToBackend(): void {
  subscribeToEvents(handleEvent);
}
