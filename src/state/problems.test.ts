import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ExtensionStatus } from '../ipc/extensions'
import type { FileProblem, ProblemsList } from '../ipc/problems'

vi.mock('../ipc/problems', () => ({ listFileProblems: vi.fn() }))

const ipc = await import('../ipc/problems')
const { documentsStore } = await import('./documents')
const {
  clearReported,
  countFileProblems,
  countProblems,
  currentProblems,
  fileProblemsStore,
  followFileProblems,
  recordProblem,
  reportedProblems,
} = await import('./problems')

const extension = (overrides: Partial<ExtensionStatus>): ExtensionStatus =>
  ({
    id: 'example.tidy',
    name: 'Tidy',
    state: 'active',
    error: null,
    configErrors: [],
    ...overrides,
  }) as ExtensionStatus

describe('problems', () => {
  beforeEach(() => reportedProblems.set([]))

  it('lists current problems from extensions and the settings file', () => {
    const problems = currentProblems(
      [
        extension({ state: 'failed', error: 'trap: unreachable' }),
        extension({
          id: 'example.todos',
          name: 'TODOs',
          configErrors: ['limit must be at least 1; using the default'],
        }),
        extension({ id: 'C:/x/broken', name: 'broken', state: 'invalid', error: 'no manifest' }),
        extension({ id: 'example.fine', name: 'Fine' }),
      ],
      'unknown field `bogus`',
    )
    expect(problems.map(p => [p.source, p.severity, p.message, p.extension])).toEqual([
      ['Settings', 'error', "settings.toml isn't in effect: unknown field `bogus`", null],
      ['Tidy', 'error', 'Stopped: trap: unreachable', 'example.tidy'],
      ['TODOs', 'warning', 'Setting limit must be at least 1; using the default', 'example.todos'],
      ['broken', 'error', "Couldn't be loaded: no manifest", null],
    ])
    expect(problems.every(p => p.current && p.time === null)).toBe(true)
    expect(countProblems(problems)).toEqual({ errors: 3, warnings: 1 })
  })

  it('records reported problems newest first, counting repeats', () => {
    recordProblem('warning', 'Saving failed: disk full', 'Ferrite', null, 1000)
    recordProblem('error', 'Lint crashed', 'Tidy', 'example.tidy', 2000)
    recordProblem('warning', 'Saving failed: disk full', 'Ferrite', null, 3000)
    expect(reportedProblems.get().map(p => [p.message, p.count, p.time])).toEqual([
      ['Saving failed: disk full', 2, 3000],
      ['Lint crashed', 1, 2000],
    ])
    // The same text from another source is another problem.
    recordProblem('warning', 'Lint crashed', 'TODOs')
    expect(reportedProblems.get()).toHaveLength(3)

    clearReported(reportedProblems.get()[0]!.key)
    expect(reportedProblems.get().map(p => p.source)).toEqual(['Ferrite', 'Tidy'])
    clearReported()
    expect(reportedProblems.get()).toEqual([])
  })

  it('keeps the most recent 200 reported problems', () => {
    for (let i = 0; i < 205; i++) {
      recordProblem('warning', `problem ${i}`)
    }
    expect(reportedProblems.get()).toHaveLength(200)
    expect(reportedProblems.get()[0]!.message).toBe('problem 204')
  })
})

describe('file problems', () => {
  const at = (severity: FileProblem['severity']): FileProblem => ({
    start: { line: 0, column: 0 },
    end: { line: 0, column: 1 },
    severity,
    message: 'm',
    source: null,
  })
  const list = (...severities: FileProblem['severity'][]): ProblemsList => ({
    files: [{ path: '/a.rs', document: 1, title: 'a.rs', problems: severities.map(at) }],
    leftOut: 0,
  })

  it('counts errors and warnings, not information', () => {
    expect(countFileProblems(list(1, 2, 2, 3))).toEqual({ errors: 1, warnings: 2 })
  })

  it('fetches once for a burst of changes, and when documents open or close', async () => {
    vi.useFakeTimers()
    try {
      vi.mocked(ipc.listFileProblems).mockResolvedValue(list(1))
      const stop = followFileProblems()
      await vi.advanceTimersByTimeAsync(300)
      // Fetched when following starts.
      const fetched = vi.mocked(ipc.listFileProblems).mock.calls.length
      expect(fileProblemsStore.get()).toEqual(list(1))

      vi.mocked(ipc.listFileProblems).mockResolvedValue(list(2))
      documentsStore.set({ documents: [{ id: 1 } as never], activeId: 1 })
      documentsStore.set({ documents: [{ id: 1 } as never, { id: 2 } as never], activeId: 2 })
      // Switching documents doesn't change whose problems are listed.
      documentsStore.set({ documents: [{ id: 1 } as never, { id: 2 } as never], activeId: 1 })
      await vi.advanceTimersByTimeAsync(300)
      expect(vi.mocked(ipc.listFileProblems).mock.calls.length).toBe(fetched + 1)
      expect(fileProblemsStore.get()).toEqual(list(2))
      stop()
    } finally {
      vi.useRealTimers()
      documentsStore.set({ documents: [], activeId: null })
    }
  })
})
