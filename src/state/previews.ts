/**
 * Extension previews as the workbench sees them: which are open (the dock
 * gives each a tab, as it does documents and terminals), and a version per
 * preview, bumped when its content changes so its tab reloads it.
 *
 * The backend owns what's open: an extension opens a preview, and it closes
 * when its document or extension does, or when the user closes its tab.
 */
import { listPreviews, previewClosed, type PreviewInfo } from '../ipc/extensions'
import { reportError } from './notifications'
import { createStore } from './store'
import { webviewClosed } from './webviews'

export const previewsStore = createStore<readonly PreviewInfo[]>([])
/** Bumped per preview when its content changes. */
export const previewVersions = createStore<Readonly<Record<string, number>>>({})

/** Previews an extension asked to show beside their document, until placed. */
const besideRequests = new Set<string>()
/** Shows a preview once it's listed; set by the dock, which imports this module. */
let showHandler: ((id: string, beside: boolean) => void) | null = null

export function setPreviewHandler(show: (id: string, beside: boolean) => void): void {
  showHandler = show
}

/** Whether a newly listed preview was asked to open beside its document (asked once). */
export function takeBesideRequest(id: string): boolean {
  return besideRequests.delete(id)
}

export async function refreshPreviews(): Promise<void> {
  try {
    previewsStore.set(await listPreviews())
  } catch (error) {
    reportError('Loading previews', error)
  }
}

export function previewChanged(id: string): void {
  previewVersions.update(versions => ({ ...versions, [id]: (versions[id] ?? 0) + 1 }))
}

/** An extension asked to show a preview: once it's listed, the dock shows it. */
export async function showPreviewRequested(id: string, beside: boolean): Promise<void> {
  const open = previewsStore.get().some(preview => preview.id === id)
  if (beside && !open) {
    besideRequests.add(id)
  }
  await refreshPreviews()
  besideRequests.delete(id)
  if (previewsStore.get().some(preview => preview.id === id)) {
    showHandler?.(id, beside)
  }
}

/** The user closed a preview's tab. */
export function closePreview(id: string): void {
  previewsStore.update(previews => previews.filter(preview => preview.id !== id))
  webviewClosed(id)
  previewClosed(id).catch((error: unknown) => reportError('Closing the preview', error))
}
