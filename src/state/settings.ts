/**
 * The settings editor's state: every setting with its value (from the
 * backend, which owns `settings.toml`), and the search it opens with.
 *
 * Changes are written to `settings.toml` one setting at a time; the backend
 * applies them and announces the new settings, which reloads these.
 */
import {
  getSettingsSchema,
  setSetting,
  type SettingValue,
  type SettingsSchema,
} from '../ipc/workspace'
import { reportError } from './notifications'
import { createStore } from './store'
import { showPage } from './viewRequests'

/** `null` until loaded. */
export const settingsSchemaStore = createStore<SettingsSchema | null>(null)
/** The search the settings editor shows; set when it's opened for something. */
export const settingsQuery = createStore<string>('')

let loading: Promise<void> | null = null
let loadAgain = false

/** Reloads the settings; coalesces calls made while one is running. */
export function refreshSettings(): Promise<void> {
  if (loading) {
    loadAgain = true
    return loading
  }
  loading = (async () => {
    do {
      loadAgain = false
      try {
        settingsSchemaStore.set(await getSettingsSchema())
      } catch (error) {
        reportError('Loading settings', error)
      }
    } while (loadAgain)
    loading = null
  })()
  return loading
}

/** Reloads the settings if the settings editor has loaded them. */
export function refreshSettingsIfLoaded(): void {
  if (settingsSchemaStore.get() !== null) {
    void refreshSettings()
  }
}

/**
 * Sets a setting in `settings.toml`, or removes it (`null`) so its default
 * applies. Resolves once it's applied; a rejected change is reported.
 */
export async function changeSetting(path: string[], value: SettingValue | null): Promise<void> {
  try {
    await setSetting(path, value)
  } catch (error) {
    reportError('Changing the setting', error)
  }
  await refreshSettings()
}

/** Opens the settings editor, searching for `query` (e.g. `@ext:<id>`). */
export function openSettings(query = ''): void {
  settingsQuery.set(query)
  showPage('settings')
  // It may already be open, showing settings from before the file changed.
  refreshSettingsIfLoaded()
}

/** An extension's settings: the query that shows only them. */
export const extensionQuery = (id: string) => `@ext:${id}`
