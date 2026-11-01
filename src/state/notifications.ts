import { CommandError, log } from '../ipc'
import { recordProblem } from './problems'
import { createStore } from './store'

export interface NotificationAction {
  label: string
  run: () => void | Promise<void>
}

export interface Notification {
  id: number
  kind: 'error' | 'warning' | 'info'
  message: string
  /** Buttons; choosing one dismisses the notification. */
  actions?: readonly NotificationAction[]
  /** Stays until acted on or dismissed, e.g. a question only it asks. */
  sticky?: boolean
}

export interface NotifyOptions {
  /** Stays until acted on or dismissed; others disappear after a while. */
  sticky?: boolean
  /**
   * Whether a warning or error is also listed in the Problems view (the
   * default), so it can be found after the notification is gone.
   */
  record?: boolean
  /** What it's about, for the Problems view: `Ferrite` by default. */
  source?: string
  /** The id of the extension it's about. */
  extension?: string | null
}

export const notifications = createStore<readonly Notification[]>([])

let nextId = 1

export function notify(
  kind: Notification['kind'],
  message: string,
  actions?: readonly NotificationAction[],
  options: NotifyOptions = {},
): number {
  const id = nextId++
  const sticky = options.sticky ?? false
  notifications.update(current => [...current, { id, kind, message, actions, sticky }])
  if (kind !== 'info' && options.record !== false) {
    recordProblem(kind, message, options.source, options.extension ?? null)
  }
  return id
}

export function dismiss(id: number): void {
  notifications.update(current => current.filter(n => n.id !== id))
}

/** Shows a failed action to the user and records it in the log. */
export function reportError(action: string, error: unknown): void {
  const message = error instanceof CommandError ? error.message : String(error)
  log('warn', `${action} failed: ${message}`)
  notify('error', `${action} failed: ${message}`)
}
