import { CircleX, ListX, TriangleAlert, X } from 'lucide-react'
import { executeCommand } from '../commands/registry'
import { restartExtension } from '../ipc/extensions'
import { reportError } from '../state/notifications'
import { APP_CHANNEL, extensionChannel, showOutput } from '../state/output'
import { clearReported, reportedProblems, useProblems, type Problem } from '../state/problems'
import { extensionQuery, openSettings } from '../state/settings'
import { useStore } from '../state/store'
import './ProblemsPanel.css'

interface Group {
  source: string
  problems: Problem[]
}

/** Problems grouped by source, in the order they're listed. */
function groupsOf(problems: readonly Problem[]): Group[] {
  const groups = new Map<string, Problem[]>()
  for (const problem of problems) {
    groups.set(problem.source, [...(groups.get(problem.source) ?? []), problem])
  }
  return [...groups].map(([source, list]) => ({ source, problems: list }))
}

const timeFormat = new Intl.DateTimeFormat(undefined, {
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
})

/**
 * Problems with Ferrite and with extensions: current ones (an extension
 * that stopped, settings that were ignored), then the warnings and errors
 * notifications reported, which stay until cleared.
 */
export function ProblemsPanel() {
  const problems = useProblems()
  if (problems.length === 0) {
    return (
      <p className="panel-empty">
        No problems with Ferrite or its extensions. Problems in files show in the editor.
      </p>
    )
  }
  return (
    <ul className="problems" aria-label="Problems">
      {groupsOf(problems).map(group => (
        <ProblemGroup key={group.source} group={group} />
      ))}
    </ul>
  )
}

function act(action: string, run: () => Promise<void> | void): void {
  Promise.resolve(run()).catch((error: unknown) => reportError(action, error))
}

function ProblemGroup({ group }: { group: Group }) {
  const { source, problems } = group
  const extension = problems.find(problem => problem.extension)?.extension ?? null
  const failed = problems.some(problem => problem.key.startsWith('failed:'))
  const settings = problems.some(problem => problem.key.startsWith('config:'))
  const headingId = `problems-${source.replace(/\W/g, '_')}`
  return (
    <li className="problems__group" aria-labelledby={headingId}>
      <div className="problems__source">
        <span id={headingId} className="problems__name">
          {source}
        </span>
        <span className="problems__count">{problems.length}</span>
        <span className="problems__actions">
          {failed && extension && (
            <button
              type="button"
              className="problems__button"
              onClick={() => act('Restarting the extension', () => restartExtension(extension))}
            >
              Restart
            </button>
          )}
          {settings && extension && (
            <button
              type="button"
              className="problems__button"
              onClick={() => openSettings(extensionQuery(extension))}
            >
              Settings
            </button>
          )}
          {source === 'Settings' && (
            <button
              type="button"
              className="problems__button"
              onClick={() => void executeCommand('preferences.openSettingsFile')}
            >
              Open settings.toml
            </button>
          )}
          {(extension || source === 'Ferrite') && (
            <button
              type="button"
              className="problems__button"
              aria-label={`Show ${source} Log`}
              onClick={() => showOutput(extension ? extensionChannel(extension) : APP_CHANNEL)}
            >
              Show Log
            </button>
          )}
        </span>
      </div>
      <ul className="problems__list">
        {problems.map(problem => (
          <ProblemItem key={problem.key} problem={problem} />
        ))}
      </ul>
    </li>
  )
}

function ProblemItem({ problem }: { problem: Problem }) {
  const Severity = problem.severity === 'error' ? CircleX : TriangleAlert
  const label = problem.severity === 'error' ? 'Error' : 'Warning'
  return (
    <li className="problem" data-severity={problem.severity}>
      <Severity
        className="problem__icon"
        size={14}
        strokeWidth={1.75}
        role="img"
        aria-label={label}
      />
      <span className="problem__message">{problem.message}</span>
      {problem.count > 1 && (
        <span className="problem__repeats" title={`Reported ${problem.count} times`}>
          ×{problem.count}
        </span>
      )}
      {problem.time !== null && (
        <time className="problem__time" dateTime={new Date(problem.time).toISOString()}>
          {timeFormat.format(problem.time)}
        </time>
      )}
      {!problem.current && (
        <button
          type="button"
          className="problem__clear"
          aria-label={`Clear: ${problem.message}`}
          title="Clear"
          onClick={() => clearReported(problem.key)}
        >
          <X size={14} strokeWidth={1.5} aria-hidden="true" />
        </button>
      )}
    </li>
  )
}

/** Buttons beside the Problems tab. */
export function ProblemsActions() {
  const reported = useStore(reportedProblems)
  return (
    <button
      type="button"
      className="dock-action"
      title="Clear Reported Problems"
      aria-label="Clear Reported Problems"
      disabled={reported.length === 0}
      onClick={() => clearReported()}
    >
      <ListX size={16} strokeWidth={1.5} aria-hidden="true" />
    </button>
  )
}
