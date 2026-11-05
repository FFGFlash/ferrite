/**
 * Which log channel the Output view shows, and showing it: Flareite's own
 * (`flareite`), an extension's (`extension:<id>`), or a language server's
 * (`server:<name>`).
 */
import { createStore } from './store'
import { showView } from './viewRequests'

export const APP_CHANNEL = 'flareite'

export const outputChannel = createStore<string>(APP_CHANNEL)

export const extensionChannel = (id: string) => `extension:${id}`

/** Shows and focuses the Output view, on `channel` if given. */
export function showOutput(channel?: string): void {
  if (channel) {
    outputChannel.set(channel)
  }
  showView('output', true)
}

/** Shows and focuses the Problems view. */
export function showProblems(): void {
  showView('problems', true)
}
