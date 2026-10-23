/**
 * The frontend's only gateway to the Rust backend.
 *
 * Components never call `invoke` directly; they use these typed wrappers, which
 * mirror the commands in `src-tauri/src/commands.rs`. Outside the desktop shell
 * (e.g. `pnpm dev` in a browser, or tests) the wrappers degrade gracefully.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";

/** Mirrors `commands::AppInfo`. */
export interface AppInfo {
  name: string;
  version: string;
  os: string;
  arch: string;
  tauriVersion: string;
}

export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

const desktop = isTauri();

export function getAppInfo(): Promise<AppInfo | null> {
  return desktop ? invoke<AppInfo>("app_info") : Promise.resolve(null);
}

/** Writes a message to the backend log (or the console outside the desktop shell). */
export function log(level: LogLevel, message: string): void {
  if (!desktop) {
    console[level === "trace" ? "debug" : level](message);
    return;
  }
  invoke("frontend_log", { level, message }).catch((error: unknown) => {
    console.error("frontend_log failed:", error, message);
  });
}

let readyReported = false;

/** Tells the backend the workbench has rendered. Only the first call is sent. */
export function notifyFrontendReady(): void {
  if (readyReported || !desktop) {
    return;
  }
  readyReported = true;
  invoke("frontend_ready").catch((error: unknown) => {
    console.error("frontend_ready failed:", error);
  });
}

/** Forwards uncaught errors and unhandled rejections to the backend log. */
export function installGlobalErrorLogging(): void {
  window.addEventListener("error", (event) => {
    log(
      "error",
      `Uncaught error: ${event.message} (${event.filename}:${event.lineno}:${event.colno})`,
    );
  });
  window.addEventListener("unhandledrejection", (event) => {
    log("error", `Unhandled rejection: ${describe(event.reason)}`);
  });
}

function describe(reason: unknown): string {
  if (reason instanceof Error) {
    return reason.stack ?? `${reason.name}: ${reason.message}`;
  }
  return String(reason);
}
