import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { SearchView } from '../ipc/editor'
import { FindWidget, type FindState } from './FindWidget'

const state = (overrides: Partial<FindState> = {}): FindState => ({
  query: { pattern: 'foo', regex: false, caseSensitive: false, wholeWord: false },
  replacement: '',
  showReplace: false,
  focusRequest: 1,
  ...overrides,
})

const result = (overrides: Partial<SearchView> = {}): SearchView => ({
  matches: [],
  total: 5,
  truncated: false,
  current: 1,
  error: null,
  ...overrides,
})

function renderWidget(findState = state(), view: SearchView | null = result()) {
  const handlers = {
    onQueryChange: vi.fn(),
    onReplacementChange: vi.fn(),
    onToggleReplace: vi.fn(),
    onNext: vi.fn(),
    onReplace: vi.fn(),
    onReplaceAll: vi.fn(),
    onClose: vi.fn(),
  }
  render(<FindWidget state={findState} result={view} {...handlers} />)
  return handlers
}

describe('FindWidget', () => {
  it('shows the match position and count', () => {
    renderWidget()
    expect(screen.getByRole('status')).toHaveTextContent('2 of 5')
  })

  it('reports missing matches and invalid patterns', () => {
    renderWidget(state(), result({ total: 0, current: null }))
    expect(screen.getByRole('status')).toHaveTextContent('No results')
  })

  it('navigates with Enter and Shift+Enter and closes with Escape', async () => {
    const user = userEvent.setup()
    const handlers = renderWidget()
    expect(screen.getByRole('textbox', { name: 'Find' })).toHaveFocus()
    await user.keyboard('{Enter}{Shift>}{Enter}{/Shift}{Escape}')
    expect(handlers.onNext.mock.calls).toEqual([[false], [true]])
    expect(handlers.onClose).toHaveBeenCalledOnce()
  })

  it('toggles options by button and Alt shortcut', async () => {
    const user = userEvent.setup()
    const handlers = renderWidget()
    await user.click(screen.getByRole('button', { name: 'Match Case' }))
    await user.click(screen.getByRole('textbox', { name: 'Find' }))
    await user.keyboard('{Alt>}r{/Alt}')
    expect(handlers.onQueryChange.mock.calls).toEqual([
      [{ caseSensitive: true }],
      [{ regex: true }],
    ])
  })

  it('replaces from the replace field', async () => {
    const user = userEvent.setup()
    const handlers = renderWidget(state({ showReplace: true }))
    await user.click(screen.getByRole('textbox', { name: 'Replace' }))
    await user.keyboard('{Enter}{Control>}{Enter}{/Control}')
    expect(handlers.onReplace).toHaveBeenCalledOnce()
    expect(handlers.onReplaceAll).toHaveBeenCalledOnce()
  })

  it('marks an invalid pattern', () => {
    renderWidget(state(), result({ error: 'invalid regular expression', total: 0, current: null }))
    expect(screen.getByRole('status')).toHaveTextContent('Invalid pattern')
  })
})
