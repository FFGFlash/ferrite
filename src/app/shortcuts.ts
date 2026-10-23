/**
 * Workbench-level keyboard shortcuts, handled before the editor sees the key.
 */
import { useEffect } from "react";
import type { KeyInput } from "../editor/keymap";
import { isMac } from "../editor/keymap";
import { close, newFile, openFile, save } from "../state/documents";

export type WorkbenchAction = "newFile" | "openFile" | "save" | "saveAs" | "close";

const bindings: Record<WorkbenchAction, { key: string; shift: boolean }> = {
  newFile: { key: "n", shift: false },
  openFile: { key: "o", shift: false },
  save: { key: "s", shift: false },
  saveAs: { key: "s", shift: true },
  close: { key: "w", shift: false },
};

export function shortcutAction(event: KeyInput, mac: boolean): WorkbenchAction | null {
  const mod = mac ? event.metaKey : event.ctrlKey;
  if (!mod || event.altKey) {
    return null;
  }
  const key = event.key.toLowerCase();
  for (const [action, binding] of Object.entries(bindings)) {
    if (binding.key === key && binding.shift === event.shiftKey) {
      return action as WorkbenchAction;
    }
  }
  return null;
}

export function shortcutLabel(action: WorkbenchAction, mac: boolean): string {
  const { key, shift } = bindings[action];
  const parts = [mac ? "⌘" : "Ctrl", ...(shift ? [mac ? "⇧" : "Shift"] : []), key.toUpperCase()];
  return parts.join(mac ? "" : "+");
}

export function runAction(action: WorkbenchAction): void {
  switch (action) {
    case "newFile":
      void newFile();
      break;
    case "openFile":
      void openFile();
      break;
    case "save":
      void save();
      break;
    case "saveAs":
      void save(undefined, true);
      break;
    case "close":
      void close();
      break;
  }
}

/** Installs the workbench shortcuts for the lifetime of the calling component. */
export function useWorkbenchShortcuts(): void {
  useEffect(() => {
    const mac = isMac();
    const onKeyDown = (event: KeyboardEvent) => {
      const action = shortcutAction(event, mac);
      if (action) {
        event.preventDefault();
        event.stopPropagation();
        runAction(action);
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);
}
