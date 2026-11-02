/**
 * A tab showing a file that isn't open as text:
 *
 * - **In an extension's viewer**, such as an image. The extension fills it
 *   in with preview content (`PreviewDocument` draws it), when the tab
 *   opens, when the file changes on disk, and when the extension becomes
 *   usable again.
 * - **A binary file** (`viewer` is `null`), which asks before opening it as
 *   text, as VS Code does, or offers a viewer that can show it.
 */
import { FileQuestion } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { reopenWith } from '../dock/dock'
import type { ItemId } from '../dock/model'
import { getPreviewImage, openViewer } from '../ipc/extensions'
import { extensionsStore } from '../state/extensions'
import { useStore } from '../state/store'
import { fileVersions, showFileTab, viewerOpened, viewersFor } from '../state/viewers'
import { PreviewDocument, useContent } from './PreviewPane'
import './PreviewPane.css'
import './ViewerPane.css'

interface ViewerPaneProps {
  item: ItemId
  viewer: string | null
  path: string
}

const fileName = (path: string) => path.split(/[/\\]/).pop() || path

export function ViewerPane({ item, viewer, path }: ViewerPaneProps) {
  return viewer === null ? (
    <BinaryFile item={item} path={path} />
  ) : (
    <ExtensionViewer item={item} viewer={viewer} path={path} />
  )
}

/** A message in place of the file, with what the user can do about it. */
function Notice({ message, children }: { message: string; children: ReactNode }) {
  return (
    <div className="viewer-notice">
      <FileQuestion className="viewer-notice__icon" size={40} strokeWidth={1} aria-hidden="true" />
      <p className="viewer-notice__message">{message}</p>
      <div className="viewer-notice__actions">{children}</div>
    </div>
  )
}

function BinaryFile({ item, path }: { item: ItemId; path: string }) {
  const extensions = useStore(extensionsStore)
  // Recomputed when the extensions change.
  const viewers = useMemo(() => viewersFor(path), [path, extensions])
  const preferred = viewers.find(viewer => viewer.priority === 'default')

  // A viewer for it became available (extensions load after the files named
  // at startup, or one was enabled): show it there.
  useEffect(() => {
    if (preferred) {
      showFileTab({ viewer: preferred.fullId, path })
    }
  }, [preferred, path])

  return (
    <Notice
      message={`${fileName(path)} isn't shown because it's either binary or uses an unsupported text encoding.`}
    >
      <button
        type="button"
        className="viewer-notice__button"
        onClick={() => void reopenWith(item, null)}
      >
        Open Anyway
      </button>
      {viewers.map(viewer => (
        <button
          key={viewer.fullId}
          type="button"
          className="viewer-notice__button viewer-notice__button--secondary"
          onClick={() => showFileTab({ viewer: viewer.fullId, path })}
        >
          Open in {viewer.title}
        </button>
      ))}
    </Notice>
  )
}

/** Why a viewer's extension can't fill it in now, or `null` if it can. */
function unavailableReason(state: string | undefined): string | null {
  switch (state) {
    case undefined:
      return "its extension isn't installed or loaded"
    case 'disabled':
      return 'its extension is turned off'
    case 'failed':
      return 'its extension stopped'
    case 'awaitingPermission':
      return 'its extension is waiting for you to allow its permissions'
    case 'invalid':
      return "its extension couldn't be loaded"
    default:
      return null
  }
}

function ExtensionViewer({ item, viewer, path }: { item: ItemId; viewer: string; path: string }) {
  const extensions = useStore(extensionsStore)
  const extension = extensions.find(candidate =>
    candidate.contributions.viewers.some(declared => declared.fullId === viewer),
  )
  // Its title, even while the extension can't be used.
  const contribution = extension?.contributions.viewers.find(declared => declared.fullId === viewer)
  const unavailable = unavailableReason(extension?.state)
  const version = useStore(fileVersions)[path] ?? 0
  const [id, setId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)

  // Asks the extension to fill it in: when shown, when the file changes,
  // and when the extension becomes usable again.
  useEffect(() => {
    if (unavailable !== null) {
      return
    }
    let cancelled = false
    openViewer(viewer, path).then(
      opened => {
        viewerOpened(item, opened)
        if (!cancelled) {
          setError(null)
          setId(opened)
        }
      },
      (failure: unknown) => {
        if (!cancelled) {
          setError(failure instanceof Error ? failure.message : String(failure))
        }
      },
    )
    return () => {
      cancelled = true
    }
  }, [item, viewer, path, unavailable, version, attempt])

  const title = contribution?.title ?? 'its viewer'
  const openAsText = (
    <button
      type="button"
      className="viewer-notice__button viewer-notice__button--secondary"
      onClick={() => void reopenWith(item, null)}
    >
      Open as Text
    </button>
  )
  if (unavailable !== null) {
    return (
      <Notice message={`${fileName(path)} opens in ${title}, but ${unavailable}.`}>
        {openAsText}
      </Notice>
    )
  }
  if (error !== null) {
    return (
      <Notice message={`${title} couldn't show ${fileName(path)}: ${error}`}>
        <button
          type="button"
          className="viewer-notice__button"
          onClick={() => setAttempt(count => count + 1)}
        >
          Try Again
        </button>
        {openAsText}
      </Notice>
    )
  }
  return id === null ? (
    <article className="preview viewer" aria-label={fileName(path)} aria-busy="true" />
  ) : (
    <ViewerContent id={id} path={path} version={version} />
  )
}

function ViewerContent({ id, path, version }: { id: string; path: string; version: number }) {
  const content = useContent(id)
  // A new function loads the pictures again: after the file changed.
  const loadImage = useCallback(
    (picture: string) => getPreviewImage(id, picture),
    // `version` is there to reload the pictures.
    [id, version],
  )
  const empty = !content || content.nodes.length === 0
  return (
    <article className="preview viewer" tabIndex={0} aria-label={fileName(path)} aria-busy={empty}>
      {!empty && <PreviewDocument content={content} loadImage={loadImage} />}
    </article>
  )
}
