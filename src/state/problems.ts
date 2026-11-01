/**
 * Problems with Ferrite itself and with extensions, for the Problems view
 * and the status bar's counts. (Problems in files are the editor's
 * diagnostics, which aren't listed here.)
 *
 * - **Current** problems come from state: an extension that stopped or
 *   couldn't be loaded, extension settings that were ignored, and a
 *   `settings.toml` that isn't in effect. They go away once fixed.
 * - **Reported** problems are the warnings and errors notifications showed
 *   (failed actions, extensions' messages, backend notices), so they can be
 *   found after the notification is gone. They stay until cleared.
 */
import { useMemo } from 'react'
import type { ExtensionStatus } from '../ipc/extensions'
import { getSettingsProblem } from '../ipc/workspace'
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
