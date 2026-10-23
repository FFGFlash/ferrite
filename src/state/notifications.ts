import { CommandError, log } from "../ipc";
import { createStore } from "./store";

export interface Notification {
  id: number;
  kind: "error" | "info";
  message: string;
}

export const notifications = createStore<readonly Notification[]>([]);

let nextId = 1;

export function notify(kind: Notification["kind"], message: string): number {
  const id = nextId++;
  notifications.update((current) => [...current, { id, kind, message }]);
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
