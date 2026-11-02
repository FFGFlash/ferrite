/**
 * Problems for the Problems view and the status bar's counts: those with
 * Ferrite itself and with extensions, and those in files.
 *
 * - **Current** problems come from state: an extension that stopped or
 *   couldn't be loaded, extension settings that were ignored, and a
 *   `settings.toml` that isn't in effect. They go away once fixed.
 * - **Reported** problems are the warnings and errors notifications showed
 *   (failed actions, extensions' messages, backend notices), so they can be
 *   found after the notification is gone. They stay until cleared.
 * - **File** problems are the diagnostics of open documents (from language
 *   servers and extensions) and those language servers reported for other
 *   files. The backend owns them; they're fetched again shortly after they
 *   change.
 */
import { useMemo } from 'react'
import type { ExtensionStatus } from '../ipc/extensions'
import { listFileProblems, type ProblemsList } from '../ipc/problems'
import { getSettingsProblem } from '../ipc/workspace'
import { documentsStore } from './documents'
import { extensionsStore } from './extensions'
import { createStore, useStore } from './store'

export type ProblemSeverity = 'error' | 'warning'

export interface Problem {
  /** The same problem keeps its key. */
  key: string
  severity: ProblemSeverity
  /** What it's about: `Ferrite`, `Settings`, or an extension's name. */
  source: string
  /** The id of the extension it's about. */
  extension: string | null
  message: string
  /** Current problems last until fixed; reported ones until cleared. */
  current: boolean
  /** When it was last reported (milliseconds since the epoch); `null` if current. */
  time: number | null
  /** How many times it was reported. */
  count: number
}

/** Reported problems kept, newest first; older ones are dropped. */
const MAX_REPORTED = 200

export const reportedProblems = createStore<readonly Problem[]>([])
/** Why `settings.toml` isn't in effect, if it isn't. */
export const settingsProblem = createStore<string | null>(null)

/** Records a warning or error; the same message from the same source is counted. */
export function recordProblem(
  severity: ProblemSeverity,
  message: string,
  source = 'Ferrite',
  extension: string | null = null,
  now = Date.now(),
): void {
  const key = `reported:${source}\n${message}`
  reportedProblems.update(problems => {
    const previous = problems.find(problem => problem.key === key)
    const problem: Problem = {
      key,
      severity: previous?.severity === 'error' ? 'error' : severity,
      source,
      extension,
      message,
      current: false,
      time: now,
      count: (previous?.count ?? 0) + 1,
    }
    return [problem, ...problems.filter(other => other.key !== key)].slice(0, MAX_REPORTED)
  })
}

/** Forgets reported problems: one, or all of them. */
export function clearReported(key?: string): void {
  reportedProblems.update(problems => (key ? problems.filter(p => p.key !== key) : []))
}

/** Checks `settings.toml` again, after it's loaded, changed, or saved. */
export async function refreshSettingsProblem(): Promise<void> {
  try {
    settingsProblem.set(await getSettingsProblem())
  } catch {
    // Outside the desktop app, or the check failed: nothing to show.
    settingsProblem.set(null)
  }
}

/** The current problems that extensions' statuses and the settings show. */
export function currentProblems(
  extensions: readonly ExtensionStatus[],
  settings: string | null,
): Problem[] {
  const current = (
    key: string,
    severity: ProblemSeverity,
    source: string,
    extension: string | null,
    message: string,
  ): Problem => ({ key, severity, source, extension, message, current: true, time: null, count: 1 })
  const problems: Problem[] = []
  if (settings) {
    problems.push(
      current('settings', 'error', 'Settings', null, `settings.toml isn't in effect: ${settings}`),
    )
  }
  for (const extension of extensions) {
    if (extension.state === 'invalid') {
      // Its id is its folder, as there's no manifest to name it.
      problems.push(
        current(
          `invalid:${extension.id}`,
          'error',
          extension.name,
          null,
          `Couldn't be loaded: ${extension.error ?? 'invalid'}`,
        ),
      )
      continue
    }
    if (extension.state === 'failed') {
      problems.push(
        current(
          `failed:${extension.id}`,
          'error',
          extension.name,
          extension.id,
          `Stopped: ${extension.error ?? 'it failed'}`,
        ),
      )
    }
    for (const error of extension.configErrors) {
      problems.push(
        current(
          `config:${extension.id}:${error}`,
          'warning',
          extension.name,
          extension.id,
          `Setting ${error}`,
        ),
      )
    }
  }
  return problems
}

/** Every problem: the current ones, then the reported ones, newest first. */
export function useProblems(): readonly Problem[] {
  const extensions = useStore(extensionsStore)
  const settings = useStore(settingsProblem)
  const reported = useStore(reportedProblems)
  return useMemo(
    () => [...currentProblems(extensions, settings), ...reported],
    [extensions, settings, reported],
  )
}

export function countProblems(problems: readonly Problem[]): { errors: number; warnings: number } {
  const errors = problems.filter(problem => problem.severity === 'error').length
  return { errors, warnings: problems.length - errors }
}

export const fileProblemsStore = createStore<ProblemsList>({ files: [], leftOut: 0 })

/** Waits this long after a change, so a burst of changes (typing) fetches once. */
const FILE_REFRESH_DELAY_MS = 250

let refreshTimer: ReturnType<typeof setTimeout> | null = null
let fetching: Promise<void> | null = null
let fetchAgain = false

/** Fetches file problems again soon, once for a burst of calls. */
export function fileProblemsChanged(): void {
  if (refreshTimer !== null) {
    clearTimeout(refreshTimer)
  }
  refreshTimer = setTimeout(() => {
    refreshTimer = null
    void refreshFileProblems()
  }, FILE_REFRESH_DELAY_MS)
}

/** Fetches file problems now; a call while fetching fetches once more after. */
export function refreshFileProblems(): Promise<void> {
  if (fetching) {
    fetchAgain = true
    return fetching
  }
  fetching = (async () => {
    do {
      fetchAgain = false
      try {
        fileProblemsStore.set(await listFileProblems())
      } catch {
        // Outside the desktop app: there are none.
      }
    } while (fetchAgain)
    fetching = null
  })()
  return fetching
}

/**
 * Fetches file problems now, and again whenever a document opens or closes
 * (which changes whose diagnostics are listed). Call once at startup; the
 * backend announces other changes.
 */
export function followFileProblems(): () => void {
  let documentIds = ''
  void refreshFileProblems()
  return documentsStore.subscribe(() => {
    const ids = documentsStore
      .get()
      .documents.map(document => document.id)
      .join(',')
    if (ids !== documentIds) {
      documentIds = ids
      fileProblemsChanged()
    }
  })
}

/** Errors and warnings in files (information isn't counted). */
export function countFileProblems(list: ProblemsList): { errors: number; warnings: number } {
  let errors = 0
  let warnings = 0
  for (const file of list.files) {
    for (const problem of file.problems) {
      if (problem.severity === 1) {
        errors++
      } else if (problem.severity === 2) {
        warnings++
      }
    }
  }
  return { errors, warnings }
}

/** Every error and warning: Ferrite's, extensions', and files'. */
export function useProblemCounts(): { errors: number; warnings: number } {
  const app = countProblems(useProblems())
  const files = countFileProblems(useStore(fileProblemsStore))
  return { errors: app.errors + files.errors, warnings: app.warnings + files.warnings }
}
