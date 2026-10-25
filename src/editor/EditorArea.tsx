import { memo, useEffect, useRef, useState, type DragEvent } from 'react'
import { ErrorBoundary } from '../components/ErrorBoundary'
import type { AppInfo } from '../ipc'
import type { DocumentSummary } from '../ipc/editor'
import { activate, close, documentsStore, moveDocument } from '../state/documents'
import { useStore } from '../state/store'
import { EditorView } from './EditorView'
import { activeEditor } from './activeEditor'
import { Breadcrumbs } from './Breadcrumbs'
import { WelcomePage } from './WelcomePage'
import './EditorArea.css'

interface EditorAreaProps {
  appInfo: AppInfo | null
}

export const EditorArea = memo(function EditorArea({ appInfo }: EditorAreaProps) {
  const { documents, activeId } = useStore(documentsStore)
  const active = documents.find(document => document.id === activeId)

  return (
    <main className="editor-area">
      {documents.length > 0 && (
        <div className="editor-tabs" role="tablist" aria-label="Open editors">
          {documents.map((document, index) => (
            <EditorTab
              key={document.id}
              document={document}
              index={index}
              active={document.id === activeId}
            />
          ))}
        </div>
      )}
      {active ? (
        <section
          className="editor-area__content"
          role="tabpanel"
          id={`editor-panel-${active.id}`}
          aria-labelledby={`editor-tab-${active.id}`}
        >
          <Breadcrumbs document={active} />
          <ErrorBoundary name="The editor" resetKey={active.id}>
            <EditorView key={active.id} documentId={active.id} title={active.title} />
          </ErrorBoundary>
        </section>
      ) : (
        <section className="editor-area__content editor-area__content--scroll">
          <WelcomePage appInfo={appInfo} />
        </section>
      )}
    </main>
  )
})

interface EditorTabProps {
  document: DocumentSummary
  index: number
  active: boolean
}

const TAB_DRAG_TYPE = 'application/x-ferrite-tab'

function EditorTab({ document, index, active }: EditorTabProps) {
  const ref = useRef<HTMLDivElement>(null)
  const [drop, setDrop] = useState<'before' | 'after' | null>(null)
  const label = document.dirty ? `${document.title}, unsaved` : document.title

  useEffect(() => {
    if (active) {
      ref.current?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
    }
  }, [active])

  const sideOf = (event: DragEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect()
    return event.clientX < rect.left + rect.width / 2 ? 'before' : 'after'
  }

  return (
    <div
      ref={ref}
      className="editor-tab"
      draggable
      data-drop={drop ?? undefined}
      onDragStart={event => {
        event.dataTransfer.setData(TAB_DRAG_TYPE, String(document.id))
        event.dataTransfer.effectAllowed = 'move'
      }}
      onDragOver={event => {
        if (event.dataTransfer.types.includes(TAB_DRAG_TYPE)) {
          event.preventDefault()
          setDrop(sideOf(event))
        }
      }}
      onDragLeave={() => setDrop(null)}
      onDrop={event => {
        setDrop(null)
        const id = Number(event.dataTransfer.getData(TAB_DRAG_TYPE))
        if (!Number.isFinite(id) || id === document.id) {
          return
        }
        event.preventDefault()
        const { documents } = documentsStore.get()
        const from = documents.findIndex(d => d.id === id)
        const target = sideOf(event) === 'before' ? index : index + 1
        moveDocument(id, from < target ? target - 1 : target)
      }}
      role="tab"
      id={`editor-tab-${document.id}`}
      aria-selected={active}
      aria-controls={active ? `editor-panel-${document.id}` : undefined}
      aria-label={label}
      data-dirty={document.dirty || undefined}
      title={document.path ?? document.title}
      onMouseDown={event => {
        if (event.button === 0) {
          activate(document.id)
        }
      }}
      // The browser moves focus to the clicked tab after mousedown; hand it
      // back to the editor once the click completes.
      onClick={() => requestAnimationFrame(() => activeEditor.get()?.focus())}
      onAuxClick={event => {
        if (event.button === 1) {
          void close(document.id)
        }
      }}
    >
      <span className="editor-tab__title">{document.title}</span>
      <button
        type="button"
        className="editor-tab__close"
        aria-label={`Close ${document.title}`}
        onMouseDown={event => event.stopPropagation()}
        onClick={() => void close(document.id)}
      >
        <span className="editor-tab__dirty" aria-hidden="true" />
        <span className="editor-tab__x" aria-hidden="true">
          ×
        </span>
      </button>
    </div>
  )
}
