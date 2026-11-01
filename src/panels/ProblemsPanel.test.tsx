import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ExtensionStatus } from '../ipc/extensions'

vi.mock('../ipc/extensions', () => ({ restartExtension: vi.fn(async () => {}) }))
vi.mock('../commands/registry', () => ({ executeCommand: vi.fn(async () => {}) }))
vi.mock('../state/viewRequests', () => ({ showView: vi.fn(), showPage: vi.fn() }))

const { axeViolations } = await import('../test/axe')
const ipc = await import('../ipc/extensions')
const { executeCommand } = await import('../commands/registry')
const { showView } = await import('../state/viewRequests')
const { extensionsStore } = await import('../state/extensions')
const { outputChannel } = await import('../state/output')
const { recordProblem, reportedProblems, settingsProblem } = await import('../state/problems')
const { ProblemsActions, ProblemsPanel } = await import('./ProblemsPanel')

const tidy = {
  id: 'example.tidy',
  name: 'Tidy',
  state: 'failed',
  error: 'trap: unreachable',
  configErrors: ['width must be at least 0; using the default'],
} as ExtensionStatus

/** The group of problems from `source`. */
const group = (source: string) => screen.getByRole('listitem', { name: source }) as HTMLElement

describe('ProblemsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    extensionsStore.set([])
    reportedProblems.set([])
    settingsProblem.set(null)
    outputChannel.set('ferrite')
  })

  it('says when there are no problems', () => {
    render(<ProblemsPanel />)
    expect(screen.getByText(/No problems with Ferrite or its extensions/)).toBeTruthy()
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
      'Ferrite',
    ])
    expect(
      within(group('Tidy'))
        .getAllByRole('img')
        .map(icon => icon.getAttribute('aria-label')),
    ).toEqual(['Error', 'Warning', 'Error'])
    expect(within(group('Ferrite')).getByText('×2')).toBeTruthy()
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
    await user.click(within(group('Ferrite')).getByRole('button', { name: /^Clear:/ }))
    expect(screen.queryByRole('listitem', { name: 'Ferrite' })).toBeNull()
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
