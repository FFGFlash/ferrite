/**
 * Tells extensions which file the editor in use shows, as it changes, for
 * views that follow it (Git's File History). It's the current group's tab:
 * a document's file, a viewer's, or a diff's modified file. A tab that
 * isn't an editor's (a view, a terminal) leaves it as it was, so moving to
 * the side bar keeps the file, as in VS Code.
 */
import { setActiveFile } from '../ipc/extensions'
import { diffsStore, isAddress, parseDiffItem } from '../state/diffs'
import { documentsStore } from '../state/documents'
import { reportError } from '../state/notifications'
import { currentGroup, dockStore, parseDocumentItem, parseViewerItem } from './dock'
import { findGroup, itemKind } from './model'

/** How long the file must stay before extensions are told. */
const SETTLE_MS = 150

/** The file the current tab shows; `undefined` for a tab that isn't an editor's. */
function currentFile(): string | null | undefined {
  const item = findGroup(dockStore.get(), currentGroup.get())?.active
  if (!item) {
    return null
  }
  switch (itemKind(item)) {
    case 'document': {
      const { document } = parseDocumentItem(item)
      return (
        documentsStore.get().documents.find(candidate => candidate.id === document)?.path ?? null
      )
    }
    case 'viewer':
      return parseViewerItem(item).path
    case 'diff': {
      const modified = parseDiffItem(item)?.modified
      return modified && !isAddress(modified) ? modified : null
    }
    case 'preview':
    case 'page':
      return null
    default:
      return undefined
  }
}

let sent: string | null = null
let timer: ReturnType<typeof setTimeout> | undefined

function check(): void {
  clearTimeout(timer)
  timer = setTimeout(() => {
    const file = currentFile()
    if (file === undefined || file === sent) {
      return
    }
    sent = file
    setActiveFile(file).catch((error: unknown) => reportError('Following the active file', error))
  }, SETTLE_MS)
}

/** Starts telling extensions; returns a function that stops. */
export function followActiveFile(): () => void {
  const stops = [
    dockStore.subscribe(check),
    currentGroup.subscribe(check),
    documentsStore.subscribe(check),
    diffsStore.subscribe(check),
  ]
  check()
  return () => {
    clearTimeout(timer)
    for (const stop of stops) {
      stop()
    }
  }
}
