/**
 * Typed wrappers for the extension commands in `src-tauri/src/extensions.rs`.
 */
import { call } from "./index";

export type Permission = "documents-read" | "documents-write";

export type ExtensionState =
  "inactive" | "awaitingPermission" | "starting" | "active" | "failed" | "disabled" | "invalid";

export interface PermissionStatus {
  permission: Permission;
  description: string;
  granted: boolean;
  /** Whether the user has granted or denied it. */
  decided: boolean;
}

export interface ExtensionCommand {
  id: string;
  title: string;
  category: string;
}

export interface ExtensionStatus {
  id: string;
  name: string;
  version: string;
  description: string;
  /** The extension's folder. */
  path: string;
  development: boolean;
  state: ExtensionState;
  error: string | null;
  permissions: PermissionStatus[];
  commands: ExtensionCommand[];
  configErrors: string[];
}

export type MessageLevel = "info" | "warning" | "error";

export const listExtensions = () => call<ExtensionStatus[]>("extensions_list");
export const reloadExtensions = () => call<void>("extensions_reload");
/** Runs an extension command for the active document; resolves when it finishes. */
export const runExtensionCommand = (command: string, documentId: number | null) =>
  call<void>("extension_run_command", { command, documentId });
/** Grants `granted` and denies the extension's other requested permissions. */
export const setExtensionPermissions = (id: string, granted: Permission[]) =>
  call<void>("extension_set_permissions", { id, granted });
export const setExtensionEnabled = (id: string, enabled: boolean) =>
  call<void>("extension_set_enabled", { id, enabled });
export const restartExtension = (id: string) => call<void>("extension_restart", { id });
/** Asks for a folder and loads it as a development extension. */
export const addDevelopmentExtension = () => call<string | null>("extension_add_development");
