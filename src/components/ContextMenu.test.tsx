import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ContextMenu } from './ContextMenu'
import { closeContextMenu, contextMenuStore, openContextMenu } from './contextMenuState'

describe('ContextMenu', () => {
  afterEach(() => act(() => closeContextMenu()))

  it('drops extra separators and runs the chosen action', async () => {
    const user = userEvent.setup()
    const copy = vi.fn()
    render(<ContextMenu />)
    act(() =>
      openContextMenu(10, 10, [
        null,
        { label: 'Cut', disabled: true, run: vi.fn() },
        { label: 'Copy', hint: 'Ctrl+C', run: copy },
        null,
        null,
      ]),
    )
    expect(screen.getAllByRole('menuitem')).toHaveLength(2)
    expect(screen.queryAllByRole('separator')).toHaveLength(0)
    // The disabled item is skipped: Enter runs Copy.
    await user.keyboard('{Enter}')
    expect(copy).toHaveBeenCalled()
    expect(contextMenuStore.get()).toBeNull()
  })

  it('closes with Escape without running anything', async () => {
    const user = userEvent.setup()
    const run = vi.fn()
    render(<ContextMenu />)
    act(() => openContextMenu(10, 10, [{ label: 'Open', run }]))
    await user.keyboard('{Escape}')
    expect(run).not.toHaveBeenCalled()
    expect(screen.queryByRole('menu')).toBeNull()
  })
})
