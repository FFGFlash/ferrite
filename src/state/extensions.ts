/**
 * Extensions as the workbench sees them: their status (for the Extensions
 * view), their commands (registered in the command registry, so they appear
 * in the palette), permission prompts, and their messages and failures.
 */
import { registerCommands, type Command } from "../commands/registry";
import { activeEditor } from "../editor/activeEditor";
import {
  listExtensions,
  restartExtension,
  runExtensionCommand,
  setExtensionPermissions,
  type ExtensionStatus,
  type MessageLevel,
} from "../ipc/extensions";
import { updateLayout } from "./layout";
import { notify, reportError } from "./notifications";
import { createStore } from "./store";

export const extensionsStore = createStore<readonly ExtensionStatus[]>([]);

let unregisterCommands: (() => void) | null = null;
/** Extensions already asked about permissions this session. */
const prompted = new Set<string>();
let refreshing: Promise<void> | null = null;
let refreshAgain = false;

export function showExtensionsView(): void {
  updateLayout({ activeView: "extensions", sidebarVisible: true });
}

/** Reloads extension statuses; coalesces calls made while one is running. */
export function refreshExtensions(): Promise<void> {
  if (refreshing) {
    refreshAgain = true;
    return refreshing;
  }
  refreshing = (async () => {
    do {
      refreshAgain = false;
      try {
        applyStatuses(await listExtensions());
      } catch (error) {
        reportError("Loading extensions", error);
      }
    } while (refreshAgain);
    refreshing = null;
  })();
  return refreshing;
}

export function applyStatuses(statuses: readonly ExtensionStatus[]): void {
  extensionsStore.set(statuses);
  registerExtensionCommands(statuses);
  promptForPermissions(statuses);
}

function registerExtensionCommands(statuses: readonly ExtensionStatus[]): void {
  unregisterCommands?.();
  const commands: Command[] = statuses
    .filter((extension) => extension.state !== "disabled" && extension.state !== "invalid")
    .flatMap((extension) =>
      extension.commands.map((command) => ({
        id: command.id,
        title: command.title,
        category: command.category,
        run: () => runExtensionCommand(command.id, activeEditor.get()?.documentId ?? null),
      })),
    );
  unregisterCommands = registerCommands(commands);
}

function promptForPermissions(statuses: readonly ExtensionStatus[]): void {
  for (const extension of statuses) {
    if (extension.state !== "awaitingPermission" || prompted.has(extension.id)) {
      continue;
    }
    prompted.add(extension.id);
    const requested = extension.permissions.map((p) => p.permission);
    const wants = extension.permissions.map((p) => p.description.toLowerCase()).join("; ");
    notify("info", `The extension ${extension.name} wants to: ${wants}.`, [
      { label: "Allow", run: () => setExtensionPermissions(extension.id, requested) },
      { label: "Don't Allow", run: () => setExtensionPermissions(extension.id, []) },
      { label: "Details", run: showExtensionsView },
    ]);
  }
}

export function showExtensionMessage(
  extension: string,
  level: MessageLevel,
  message: string,
): void {
  notify(level, `${extension}: ${message}`);
}

export function showExtensionFailure(id: string, name: string, reason: string): void {
  notify("error", `The extension ${name} stopped: ${reason}`, [
    { label: "Restart", run: () => restartExtension(id) },
    { label: "Details", run: showExtensionsView },
  ]);
}
