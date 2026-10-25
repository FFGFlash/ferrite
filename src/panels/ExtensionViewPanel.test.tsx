import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ViewContent, ViewEntry } from '../ipc/extensions'

vi.mock('../ipc/extensions', () => ({
  getViewContent: vi.fn(),
  viewOpened: vi.fn(() => Promise.resolve()),
  runExtensionCommand: vi.fn(() => Promise.resolve()),
}))

const ipc = await import('../ipc/extensions')
const { viewChanged } = await import('../state/extensions')
const { ExtensionViewPanel } = await import('./ExtensionViewPanel')

const entry = (
  id: string,
  parent: string | null,
  overrides: Partial<ViewEntry> = {},
): ViewEntry => ({
  id,
  parent,
  label: id,
  description: null,
  tooltip: null,
  expanded: false,
  command: null,
  argument: null,
  ...overrides,
})

describe('ExtensionViewPanel', () => {
  beforeEach(() => vi.clearAllMocks())

  it("shows a tree, expands it, and runs an item's command with its argument", async () => {
    const user = userEvent.setup()
    const content: ViewContent = {
      message: '2 TODOs',
      items: [
        entry('main.rs', null, { expanded: true, description: '2' }),
        entry('todo-1', 'main.rs', { command: 'example.todos.reveal', argument: '1:4' }),
        entry('lib.rs', null),
        entry('todo-2', 'lib.rs'),
      ],
    }
    vi.mocked(ipc.getViewContent).mockResolvedValue(content)
    render(<ExtensionViewPanel view="example.todos.list" />)
    expect(await screen.findByText('2 TODOs')).toBeInTheDocument()
    expect(ipc.viewOpened).toHaveBeenCalledWith('example.todos.list')
    expect(screen.getByText('todo-1')).toBeInTheDocument()
    expect(screen.queryByText('todo-2')).toBeNull()

    await user.click(screen.getByText('lib.rs'))
    expect(screen.getByText('todo-2')).toBeInTheDocument()

    await user.click(screen.getByText('todo-1'))
    expect(ipc.runExtensionCommand).toHaveBeenCalledWith('example.todos.reveal', null, '1:4')
  })

  it('reloads when the extension changes the view', async () => {
    vi.mocked(ipc.getViewContent).mockResolvedValue({ message: null, items: [] })
    render(<ExtensionViewPanel view="example.todos.list" />)
    await act(async () => {})
    vi.mocked(ipc.getViewContent).mockResolvedValue({ message: 'Updated', items: [] })
    act(() => viewChanged('example.todos.list'))
    expect(await screen.findByText('Updated')).toBeInTheDocument()
  })
})
