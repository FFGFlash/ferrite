/**
 * Viewers: tabs that show a file the way an extension draws it (an image,
 * say) rather than as text, and the tab that asks before opening a binary
 * file as text. Which viewers a file can open in, and which it opens in by
 * default, come from extensions' `[[viewers]]`.
 *
 * The dock owns the tabs; it registers a handler here (as it does for views
 * and pages), so the documents store can open a file in a viewer without
 * importing the dock.
 */
import { previewClosed, type ViewerContribution } from '../ipc/extensions'
import { extensionsStore, patternMatcher, usableExtensions } from './extensions'
import { reportError } from './notifications'
import { createStore } from './store'

/** A file shown in a tab without being a text document. */
export interface FileTab {
  /** The viewer's full id, or `null` for a binary file waiting to be opened as text. */
  viewer: string | null
  path: string
}

const fileName = (path: string) => path.split(/[/\\]/).pop() ?? path

/** The viewers of usable extensions that can show `path`, defaults first. */
export function viewersFor(path: string): ViewerContribution[] {
  const name = fileName(path)
  const matching = usableExtensions(extensionsStore.get()).flatMap(extension =>
    extension.contributions.viewers.filter(viewer =>
      viewer.patterns.some(pattern => patternMatcher(pattern).test(name)),
    ),
  )
  return [
    ...matching.filter(viewer => viewer.priority === 'default'),
    ...matching.filter(viewer => viewer.priority !== 'default'),
  ]
}

/** The viewer `path` opens in, unless the user chooses another way. */
export function defaultViewerFor(path: string): ViewerContribution | null {
  return viewersFor(path).find(viewer => viewer.priority === 'default') ?? null
}

/** A viewer by its full id, if an extension that can be used declares it. */
export function findViewer(id: string): ViewerContribution | null {
  for (const extension of usableExtensions(extensionsStore.get())) {
    const viewer = extension.contributions.viewers.find(candidate => candidate.fullId === id)
    if (viewer) {
      return viewer
    }
  }
  return null
}

/**
 * Shows a file tab: the existing tab for `path` if there is one and `tab`
 * is `null`, else a new one. Resolves to whether one was shown.
 */
type FileTabHandler = (path: string, tab: FileTab | null) => boolean

let handler: FileTabHandler | null = null

export function setFileTabHandler(show: FileTabHandler): void {
  handler = show
}

/** Shows the tab already showing `path` without a document; false if there's none. */
export function showExistingFileTab(path: string): boolean {
  return handler?.(path, null) ?? false
}

export function showFileTab(tab: FileTab): void {
  handler?.(tab.path, tab)
}

/** Bumped per path when the file changes on disk, so its viewer reloads. */
export const fileVersions = createStore<Readonly<Record<string, number>>>({})

export function filesChanged(paths: readonly string[]): void {
  fileVersions.update(versions => {
    const next = { ...versions }
    for (const path of paths) {
      next[path] = (next[path] ?? 0) + 1
    }
    return next
  })
}

/** Preview ids of opened viewer tabs, by dock item, to close them in the backend. */
const opened = new Map<string, string>()

export function viewerOpened(item: string, id: string): void {
  opened.set(item, id)
}

/** A viewer's tab closed: its extension stops filling it in. */
export function viewerClosed(item: string): void {
  const id = opened.get(item)
  opened.delete(item)
  if (id) {
    previewClosed(id).catch((error: unknown) => reportError('Closing the viewer', error))
  }
}
