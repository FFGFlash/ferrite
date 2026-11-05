/**
 * The frontend's only gateway to the Rust backend.
 *
 * Components never call `invoke` directly; they use these typed wrappers, which
 * mirror the commands in `src-tauri/src/commands.rs`. Outside the desktop shell
 * (e.g. `pnpm dev` in a browser, or tests) the wrappers degrade gracefully.
 */
import { invoke, isTauri, type InvokeArgs } from '@tauri-apps/api/core'
import { getCurrentWebview } from '@tauri-apps/api/webview'

/** Mirrors `commands::AppInfo`. */
export interface AppInfo {
  name: string
  version: string
  os: string
  arch: string
  tauriVersion: string
}

export type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace'

const desktop = isTauri()

/** Whether the app runs inside the desktop shell (rather than a plain browser). */
export const isDesktop = (): boolean => desktop

/** A failed command. `code` is stable and machine-readable; `message` is for users. */
export class CommandError extends Error {
  readonly code: string

  constructor(code: string, message: string) {
    super(message)
    this.name = 'CommandError'
    this.code = code
  }
}

/** Invokes a backend command, normalizing failures to `CommandError`. */
export async function call<T>(command: string, args?: InvokeArgs): Promise<T> {
  if (!desktop) {
    throw new CommandError('unavailable', 'This action requires the Flareite desktop app.')
  }
  try {
    return await invoke<T>(command, args)
  } catch (error: unknown) {
    throw toCommandError(error)
  }
}

function toCommandError(error: unknown): CommandError {
  if (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    'message' in error &&
    typeof error.code === 'string' &&
    typeof error.message === 'string'
  ) {
    return new CommandError(error.code, error.message)
  }
  return new CommandError('internal', String(error))
}

export function getAppInfo(): Promise<AppInfo | null> {
  return desktop ? invoke<AppInfo>('app_info') : Promise.resolve(null)
}

/** Writes a message to the backend log (or the console outside the desktop shell). */
export function log(level: LogLevel, message: string): void {
  if (!desktop) {
    console[level === 'trace' ? 'debug' : level](message)
    return
  }
  invoke('frontend_log', { level, message }).catch((error: unknown) => {
    console.error('frontend_log failed:', error, message)
  })
}

/** Opens an `http:`, `https:`, or `mailto:` address with the system. */
export const openExternal = (url: string) => call<void>('open_external', { url })

let readyReported = false

/** Tells the backend the workbench has rendered. Only the first call is sent. */
export function notifyFrontendReady(): void {
  if (readyReported || !desktop) {
    return
  }
  readyReported = true
  invoke('frontend_ready').catch((error: unknown) => {
    console.error('frontend_ready failed:', error)
  })
}

/** Forwards uncaught errors and unhandled rejections to the backend log. */
export function installGlobalErrorLogging(): void {
  window.addEventListener('error', event => {
    log(
      'error',
      `Uncaught error: ${event.message} (${event.filename}:${event.lineno}:${event.colno})`,
    )
  })
  window.addEventListener('unhandledrejection', event => {
    log('error', `Unhandled rejection: ${describe(event.reason)}`)
  })
}

function describe(reason: unknown): string {
  if (reason instanceof Error) {
    return reason.stack ?? `${reason.name}: ${reason.message}`
  }
  return String(reason)
}

/** Sets the window's zoom factor (1 is 100%). Does nothing outside the desktop shell. */
export async function setWindowZoom(factor: number): Promise<void> {
  if (desktop) {
    await getCurrentWebview().setZoom(factor)
  }
}
