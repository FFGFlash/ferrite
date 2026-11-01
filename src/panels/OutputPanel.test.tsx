import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ExtensionStatus } from '../ipc/extensions'
import type { LogRecord, Logs } from '../ipc/logs'

vi.mock('../ipc/logs', () => ({
  readLogs: vi.fn(),
  logFilePath: vi.fn(async () => 'C:/logs/ferrite.2026-11-01.log'),
}))
vi.mock('../state/documents', () => ({ openPath: vi.fn(async () => {}) }))

const { axeViolations } = await import('../test/axe')
const ipc = await import('../ipc/logs')
const { openPath } = await import('../state/documents')
const { extensionsStore } = await import('../state/extensions')
const { outputChannel } = await import('../state/output')
const { OutputActions, OutputPanel, channelLabel, clearedAt } = await import('./OutputPanel')

const record = (seq: number, level: LogRecord['level'], message: string, fields = '') => ({
  seq,
  time: Date.UTC(2026, 10, 1, 12, 0, 0, seq),
  level,
  target: 'ferrite_app::workspace',
  message,
  fields,
})

/** Answers like the backend: each channel's records after `after`. */
function serve(records: Record<string, LogRecord[]>) {
  vi.mocked(ipc.readLogs).mockImplementation(
    async (channel: string, after: number): Promise<Logs> => ({
      channels: Object.entries(records).map(([id, list]) => ({ id, last: list.at(-1)?.seq ?? 0 })),
      records: (records[channel] ?? []).filter(r => r.seq > after),
    }),
  )
}

const lines = () => [...document.querySelectorAll('.output__line')].map(line => line.textContent)

describe('OutputPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    outputChannel.set('ferrite')
    clearedAt.set({})
    extensionsStore.set([{ id: 'example.tidy', name: 'Tidy' } as ExtensionStatus])
  })

  it('shows a channel’s records, and switches channels', async () => {
    const user = userEvent.setup()
    serve({
      'ferrite': [record(1, 'info', 'saved file', 'path=a.rs'), record(3, 'warn', 'slow')],
      'extension:example.tidy': [record(2, 'debug', 'checked 3 lines')],
      'server:rust-analyzer': [],
    })
    const { container } = render(<OutputPanel />)
    await waitFor(() => expect(lines()).toHaveLength(2))
    expect(lines()[0]).toMatch(/INFO  ferrite_app::workspace: saved file path=a\.rs$/)
    expect(screen.getByRole('log', { name: 'Ferrite log' })).toBeTruthy()
    const picker = screen.getByRole('combobox', { name: 'Log' })
    expect([...(picker as HTMLSelectElement).options].map(o => o.textContent)).toEqual([
      'Ferrite',
      'Tidy',
      'Language Server: rust-analyzer',
    ])
    expect(await axeViolations(container)).toEqual([])

    await user.selectOptions(picker, 'extension:example.tidy')
    await waitFor(() => expect(lines()).toHaveLength(1))
    // Extensions' lines leave out the target.
    expect(lines()[0]).toMatch(/DEBUG checked 3 lines$/)
  })

  it('filters by level and text, and clears what was shown', async () => {
    const user = userEvent.setup()
    serve({
      ferrite: [
        record(1, 'debug', 'noise'),
        record(2, 'info', 'opened folder'),
        record(3, 'error', 'oops'),
      ],
    })
    render(
      <>
        <OutputActions />
        <OutputPanel />
      </>,
    )
    await waitFor(() => expect(lines()).toHaveLength(3))
    await user.selectOptions(screen.getByRole('combobox', { name: 'Levels shown' }), 'info')
    expect(lines()).toHaveLength(2)
    await user.type(screen.getByRole('searchbox', { name: 'Filter log' }), 'FOLDER')
    expect(lines()).toHaveLength(1)
    await user.clear(screen.getByRole('searchbox', { name: 'Filter log' }))

    await user.click(screen.getByRole('button', { name: 'Clear Output' }))
    await waitFor(() => expect(lines()).toHaveLength(0))
    expect(screen.getByText('No lines match.')).toBeTruthy()

    await user.click(screen.getByRole('button', { name: 'Open Log File' }))
    expect(openPath).toHaveBeenCalledWith('C:/logs/ferrite.2026-11-01.log')
  })

  it('fetches new records while shown', async () => {
    vi.useFakeTimers()
    try {
      const ferrite = [record(1, 'info', 'first')]
      serve({ ferrite })
      render(<OutputPanel />)
      await act(() => vi.advanceTimersByTimeAsync(0))
      expect(lines()).toHaveLength(1)
      ferrite.push(record(2, 'info', 'second'))
      await act(async () => {
        await vi.advanceTimersByTimeAsync(1000)
      })
      expect(lines()).toHaveLength(2)
      expect(ipc.readLogs).toHaveBeenLastCalledWith('ferrite', 1)
    } finally {
      vi.useRealTimers()
    }
  })

  it('names channels', () => {
    expect(channelLabel('ferrite', [])).toBe('Ferrite')
    expect(channelLabel('extension:example.gone', [])).toBe('example.gone')
    expect(channelLabel('server:pyright', [])).toBe('Language Server: pyright')
  })
})
