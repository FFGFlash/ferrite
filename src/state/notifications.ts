import { CommandError, log } from "../ipc";
import { createStore } from "./store";

export interface NotificationAction {
  label: string;
  run: () => void | Promise<void>;
}

export interface Notification {
  id: number;
  kind: "error" | "warning" | "info";
  message: string;
  /** Buttons; a notification with actions stays until it is acted on or dismissed. */
  actions?: readonly NotificationAction[];
}

export const notifications = createStore<readonly Notification[]>([]);

let nextId = 1;

export function notify(
  kind: Notification["kind"],
  message: string,
  actions?: readonly NotificationAction[],
): number {
  const id = nextId++;
  notifications.update((current) => [...current, { id, kind, message, actions }]);
  return id;
}

export function dismiss(id: number): void {
  notifications.update((current) => current.filter((n) => n.id !== id));
}

/** Shows a failed action to the user and records it in the log. */
export function reportError(action: string, error: unknown): void {
  const message = error instanceof CommandError ? error.message : String(error);
  log("warn", `${action} failed: ${message}`);
  notify("error", `${action} failed: ${message}`);
}
