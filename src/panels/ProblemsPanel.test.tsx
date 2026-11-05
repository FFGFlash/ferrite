import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ExtensionStatus } from '../ipc/extensions'
import type { FileProblem, FileProblems } from '../ipc/problems'

vi.mock('../ipc/extensions', () => ({ restartExtension: vi.fn(async () => {}) }))
vi.mock('../commands/registry', () => ({ executeCommand: vi.fn(async () => {}) }))
vi.mock('../state/viewRequests', () => ({ showView: vi.fn(), showPage: vi.fn() }))
vi.mock('../state/documents', async importOriginal => ({
  ...(await importOriginal<typeof import('../state/documents')>()),
  openPath: vi.fn(async () => {}),
  revealInDocument: vi.fn(),
}))

const { axeViolations } = await import('../test/axe')
const ipc = await import('../ipc/extensions')
const { executeCommand } = await import('../commands/registry')
const { showView } = await import('../state/viewRequests')
const { openPath, revealInDocument } = await import('../state/documents')
const { extensionsStore } = await import('../state/extensions')
const { outputChannel } = await import('../state/output')
const { workspaceStore } = await import('../state/workspace')
const { fileProblemsStore, recordProblem, reportedProblems, settingsProblem } =
  await import('../state/problems')
const { ProblemsActions, ProblemsPanel, collapsedGroups, folderOf } =
  await import('./ProblemsPanel')

const tidy = {
  id: 'example.tidy',
  name: 'Tidy',
  state: 'failed',
  error: 'trap: unreachable',
  configErrors: ['width must be at least 0; using the default'],
} as ExtensionStatus

const problem = (
  severity: FileProblem['severity'],
  line: number,
  message: string,
  source: string | null = 'rust-analyzer',
): FileProblem => ({
  start: { line, column: 4 },
  end: { line, column: 9 },
  severity,
  message,
  source,
})

const mainRs: FileProblems = {
  path: 'C:\\work\\src\\main.rs',
  document: 7,
  title: 'main.rs',
  problems: [problem(1, 11, 'mismatched types'), problem(3, 2, 'line is long', 'Tidy')],
}
const libRs: FileProblems = {
  path: 'C:\\work\\src\\lib.rs',
  document: null,
  title: 'lib.rs',
  problems: [problem(2, 0, 'unused import')],
}

/** The group named `name`. */
const group = (name: string) => screen.getByRole('listitem', { name }) as HTMLElement

describe('ProblemsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    extensionsStore.set([])
    reportedProblems.set([])
    settingsProblem.set(null)
    fileProblemsStore.set({ files: [], leftOut: 0 })
    collapsedGroups.set(new Set())
    outputChannel.set('flareite')
    workspaceStore.update(state => ({
      ...state,
      info: { root: 'C:\\work', name: 'work', trust: 'trusted' },
    }))
  })

  it('says when there are no problems', () => {
    render(<ProblemsPanel />)
    expect(screen.getByText(/No problems have been found/)).toBeTruthy()
  })

  it('groups current and reported problems by source, with what to do about them', async () => {
    const user = userEvent.setup()
    extensionsStore.set([tidy])
    settingsProblem.set('unknown field `bogus`')
    recordProblem('error', 'Lint crashed', 'Tidy', 'example.tidy')
    recordProblem('warning', 'Saving failed: disk full')
    recordProblem('warning', 'Saving failed: disk full')
    const { container } = render(<ProblemsPanel />)

    const groups = within(screen.getByRole('list', { name: 'Problems' }))
      .getAllByRole('listitem')
      .filter(item => item.classList.contains('problems__group'))
    expect(groups.map(item => item.querySelector('.problems__name')?.textContent)).toEqual([
      'Settings',
      'Tidy',
      'Flareite',
    ])
    expect(
      within(group('Tidy'))
        .getAllByRole('img')
        .map(icon => icon.getAttribute('aria-label')),
    ).toEqual(['Error', 'Warning', 'Error'])
    expect(within(group('Flareite')).getByText('×2')).toBeTruthy()
    expect(await axeViolations(container)).toEqual([])

    await user.click(within(group('Tidy')).getByRole('button', { name: 'Restart' }))
    expect(ipc.restartExtension).toHaveBeenCalledWith('example.tidy')
    await user.click(within(group('Tidy')).getByRole('button', { name: 'Show Tidy Log' }))
    expect(outputChannel.get()).toBe('extension:example.tidy')
    expect(showView).toHaveBeenCalledWith('output', true)
    await user.click(within(group('Settings')).getByRole('button', { name: 'Open settings.toml' }))
    expect(executeCommand).toHaveBeenCalledWith('preferences.openSettingsFile')

    // Reported problems can be cleared; current ones stay until fixed.
    expect(within(group('Tidy')).getAllByRole('button', { name: /^Clear:/ })).toHaveLength(1)
    await user.click(within(group('Flareite')).getByRole('button', { name: /^Clear:/ }))
    expect(screen.queryByRole('listitem', { name: 'Flareite' })).toBeNull()
  })

  it('lists problems in files, by file, and shows one in the editor', async () => {
    const user = userEvent.setup()
    recordProblem('warning', 'Saving failed: disk full')
    fileProblemsStore.set({ files: [libRs, mainRs], leftOut: 3 })
    const { container } = render(<ProblemsPanel />)

    // After Flareite's own, with the folder inside the open folder.
    const names = [...container.querySelectorAll('.problems__name')].map(n => n.textContent)
    expect(names).toEqual(['Flareite', 'lib.rs', 'main.rs'])
    expect(within(group('main.rs')).getByText('src')).toBeTruthy()
    const rows = within(group('main.rs')).getAllByRole('button', { name: /mismatched|long/ })
    expect(rows.map(row => row.textContent)).toEqual([
      'mismatched typesrust-analyzer[Ln 12, Col 5]',
      'line is longTidy[Ln 3, Col 5]',
    ])
    expect(screen.getByText('3 more problems aren’t listed.')).toBeTruthy()
    expect(await axeViolations(container)).toEqual([])

    // An open document is revealed; a file that isn't open is opened there.
    await user.click(rows[0]!)
    expect(revealInDocument).toHaveBeenCalledWith(
      7,
      { line: 11, column: 4 },
      { line: 11, column: 9 },
    )
    await user.click(within(group('lib.rs')).getByRole('button', { name: /unused import/ }))
    expect(openPath).toHaveBeenCalledWith('C:\\work\\src\\lib.rs', {
      start: { line: 0, column: 4 },
      end: { line: 0, column: 9 },
    })
  })

  it('collapses groups, and moves between rows with the arrow keys', async () => {
    const user = userEvent.setup()
    fileProblemsStore.set({ files: [libRs, mainRs], leftOut: 0 })
    render(
      <>
        <ProblemsActions />
        <ProblemsPanel />
      </>,
    )
    const toggle = within(group('lib.rs')).getByRole('button', { name: /^lib\.rs/ })
    expect(toggle.getAttribute('aria-expanded')).toBe('true')
    await user.click(toggle)
    expect(toggle.getAttribute('aria-expanded')).toBe('false')
    expect(within(group('lib.rs')).queryByRole('button', { name: /unused import/ })).toBeNull()

    // Rows in order: lib.rs's header, main.rs's header, then its two problems.
    toggle.focus()
    await user.keyboard('{ArrowDown}{ArrowDown}')
    expect(document.activeElement?.textContent).toMatch(/^mismatched types/)
    await user.keyboard('{End}')
    expect(document.activeElement?.textContent).toMatch(/^line is long/)
    await user.keyboard('{Home}')
    expect(document.activeElement).toBe(toggle)

    await user.click(screen.getByRole('button', { name: 'Collapse All' }))
    expect(screen.queryByRole('button', { name: /mismatched/ })).toBeNull()
  })

  it('clears every reported problem from the tab', async () => {
    const user = userEvent.setup()
    recordProblem('warning', 'one')
    render(<ProblemsActions />)
    await user.click(screen.getByRole('button', { name: 'Clear Reported Problems' }))
    expect(reportedProblems.get()).toEqual([])
    expect(screen.getByRole('button', { name: 'Clear Reported Problems' })).toHaveProperty(
      'disabled',
      true,
    )
  })
})

describe('folderOf', () => {
  it('shows folders inside the open folder relative to it', () => {
    expect(folderOf('C:\\work\\src\\state\\a.ts', 'C:\\Work\\')).toBe('src/state')
    expect(folderOf('C:\\work\\a.ts', 'C:\\work')).toBe('')
    expect(folderOf('/elsewhere/a.ts', '/work')).toBe('/elsewhere')
    expect(folderOf('/work2/a.ts', '/work')).toBe('/work2')
    expect(folderOf('/a.ts', null)).toBe('')
  })
})
