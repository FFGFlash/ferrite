import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('../ipc/extensions', () => ({
  answerPrompt: vi.fn(() => Promise.resolve()),
}))

const ipc = await import('../ipc/extensions')
const { promptClosed, showPrompt } = await import('./prompts')
const { Palette } = await import('../components/Palette')
const { ConfirmDialog } = await import('../components/ConfirmDialog')
const { closePalette, paletteStore } = await import('../components/paletteState')

describe('extension prompts', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    act(() => closePalette())
  })

  it('offers a list in the palette, and answers with the chosen value', async () => {
    const user = userEvent.setup()
    render(<Palette />)
    act(() =>
      showPrompt({
        id: 1,
        extension: 'Git',
        type: 'pick',
        title: 'Choose a branch',
        items: [
          { label: 'main', description: 'current', value: 'refs/heads/main' },
          { label: 'feature', description: null, value: 'refs/heads/feature' },
        ],
      }),
    )
    const input = screen.getByRole('combobox', { name: 'Choose a branch' })
    await user.type(input, 'feat')
    await user.keyboard('{Enter}')
    expect(ipc.answerPrompt).toHaveBeenCalledWith(1, {
      type: 'picked',
      value: 'refs/heads/feature',
    })
    expect(ipc.answerPrompt).toHaveBeenCalledTimes(1)
  })

  it('asks for text, and says when it was cancelled', async () => {
    const user = userEvent.setup()
    render(<Palette />)
    act(() =>
      showPrompt({
        id: 2,
        extension: 'Git',
        type: 'input',
        title: 'Commit message',
        value: 'Fix',
        placeholder: null,
      }),
    )
    const input = screen.getByRole('combobox', { name: 'Commit message' })
    expect(input).toHaveValue('Fix')
    await user.type(input, ' it{Enter}')
    expect(ipc.answerPrompt).toHaveBeenCalledWith(2, { type: 'text', text: 'Fix it' })

    act(() =>
      showPrompt({
        id: 3,
        extension: 'Git',
        type: 'input',
        title: 'Branch name',
        value: '',
        placeholder: null,
      }),
    )
    await user.keyboard('{Escape}')
    expect(ipc.answerPrompt).toHaveBeenLastCalledWith(3, { type: 'cancelled' })
    expect(paletteStore.get().open).toBe(false)
  })

  it('confirms in a dialog, with the main choice last', async () => {
    const user = userEvent.setup()
    render(<ConfirmDialog />)
    act(() =>
      showPrompt({
        id: 4,
        extension: 'Git',
        type: 'confirm',
        message: 'Discard the changes to 2 files?',
        detail: 'Changes that aren’t staged are lost.',
        buttons: ['Discard Files', 'Stage Instead'],
        destructive: true,
      }),
    )
    const dialog = screen.getByRole('alertdialog', { name: 'Discard the changes to 2 files?' })
    expect(dialog).toHaveTextContent('Changes that aren’t staged are lost.')
    const labels = screen.getAllByRole('button').map(button => button.textContent)
    expect(labels).toEqual(['Cancel', 'Stage Instead', 'Discard Files'])
    await user.click(screen.getByRole('button', { name: 'Stage Instead' }))
    expect(ipc.answerPrompt).toHaveBeenCalledWith(4, { type: 'button', index: 1 })
    expect(screen.queryByRole('alertdialog')).toBeNull()

    // Withdrawn when its extension stops: no answer is sent.
    act(() =>
      showPrompt({
        id: 5,
        extension: 'Git',
        type: 'confirm',
        message: 'Delete scratch.txt?',
        detail: null,
        buttons: ['Delete File'],
        destructive: true,
      }),
    )
    act(() => promptClosed(5))
    expect(screen.queryByRole('alertdialog')).toBeNull()
    expect(ipc.answerPrompt).toHaveBeenCalledTimes(1)
  })
})
