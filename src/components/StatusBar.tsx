import { CircleX, TriangleAlert } from 'lucide-react'
import { memo } from 'react'
import { executeCommand, pendingChord } from '../commands/registry'
import type { AppInfo } from '../ipc'
import type { LanguageStatus } from '../ipc/editor'
import type { StatusItem } from '../ipc/extensions'
import { documentsStore } from '../state/documents'
import { editorStatus } from '../state/editorStatus'
import { statusItemsStore } from '../state/extensions'
import { showProblems } from '../state/output'
import { useProblemCounts } from '../state/problems'
import { useStore } from '../state/store'
import './StatusBar.css'

interface StatusBarProps {
  appInfo: AppInfo | null
}

export const StatusBar = memo(function StatusBar({ appInfo }: StatusBarProps) {
  const status = useStore(editorStatus)
  const { documents, activeId } = useStore(documentsStore)
  const active = documents.find(document => document.id === activeId)
  const items = useStore(statusItemsStore)
  const chord = useStore(pendingChord)
  const side = (alignment: StatusItem['alignment']) =>
    items
      .filter(item => item.alignment === alignment)
      .sort((a, b) => b.priority - a.priority)
      .map(item => <ExtensionItem key={item.id} item={item} />)

  return (
    <footer className="statusbar">
      <div className="statusbar__group">
        <ProblemsItem />
        {status && (
          <span className="statusbar__item">
            Ln {status.line}, Col {status.column}
            {status.selectionCount > 1 && ` (${status.selectionCount} selections)`}
          </span>
        )}
        {chord && (
          <span className="statusbar__item" role="status">
            ({chord}) was pressed. Waiting for the second key of the chord…
          </span>
        )}
        {side('left')}
      </div>
      <div className="statusbar__group">
        {side('right')}
        {status?.language && <LanguageItems status={status.language} />}
        {active && (
          <>
            <span className="statusbar__item" title="Line ending">
              {active.lineEnding}
            </span>
            {active.path === null ? (
              <span className="statusbar__item" title="Encoding">
                {active.encoding}
              </span>
            ) : (
              <button
                type="button"
                className="statusbar__item statusbar__button"
                title="Change File Encoding"
                aria-label={`Encoding: ${active.encoding}. Change File Encoding`}
                onClick={() => void executeCommand('file.changeEncoding')}
              >
                {active.encoding}
              </button>
            )}
          </>
        )}
        {appInfo && (
          <span className="statusbar__item">
            {appInfo.name} {appInfo.version}
          </span>
        )}
      </div>
    </footer>
  )
})

/** Errors and warnings in files, Ferrite, and extensions; opens the Problems view. */
function ProblemsItem() {
  const { errors, warnings } = useProblemCounts()
  const label = `Problems: ${errors} ${errors === 1 ? 'error' : 'errors'}, ${warnings} ${warnings === 1 ? 'warning' : 'warnings'}`
  return (
    <button
      type="button"
      className="statusbar__item statusbar__button statusbar__problems"
      title={label}
      aria-label={label}
      onClick={showProblems}
    >
      <CircleX size={14} strokeWidth={1.75} aria-hidden="true" /> {errors}
      <TriangleAlert size={14} strokeWidth={1.75} aria-hidden="true" /> {warnings}
    </button>
  )
}

/** A status bar item contributed by an extension; plain text, never markup. */
function ExtensionItem({ item }: { item: StatusItem }) {
  const title = item.tooltip ?? item.extension
  const command = item.command
  if (command) {
    return (
      <button
        type="button"
        className="statusbar__item statusbar__button"
        title={title}
        onClick={() => void executeCommand(command)}
      >
        {item.text}
      </button>
    )
  }
  return (
    <span className="statusbar__item" title={title}>
      {item.text}
    </span>
  )
}

function serverText(server: NonNullable<LanguageStatus['server']>): string {
  switch (server.state) {
    case 'starting':
      return `${server.name}: starting…`
    case 'busy':
      return `${server.name}: ${server.message ?? 'working'}…`
    case 'failed':
      return `${server.name}: unavailable`
    case 'ready':
      return server.name
  }
}

/** The active document's language and language server. (Problem counts cover every file.) */
function LanguageItems({ status }: { status: LanguageStatus }) {
  const { server } = status
  return (
    <>
      <span className="statusbar__item" title="Language">
        {status.language}
      </span>
      {server && (
        <span
          className="statusbar__item"
          data-state={server.state}
          title={server.message ?? `Language server: ${server.name}`}
        >
          {serverText(server)}
        </span>
      )}
    </>
  )
}
