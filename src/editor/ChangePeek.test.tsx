import { act, fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { Hunk, WireView as View } from '../ipc/editor'
import type { EditorTransport } from './session'

vi.mock('../ipc/editor', async importOriginal => ({
  ...(await importOriginal<typeof import('../ipc/editor')>()),
  quickDiffHunk: vi.fn(),
  quickDiffRevert: vi.fn(),
  quickDiffApply: vi.fn(),
}))

const ipc = await import('../ipc/editor')
const { EditorView } = await import('./EditorView')

function makeView(lines: string[], overrides: Partial<View> = {}): View {
  return {
    documentId: 7,
    version: 1,
    lineCount: lines.length,
    rowCount: lines.length,
    startRow: 0,
    startLine: 0,
    endLine: lines.length,
    visible: [[0, lines.length]],
    folded: [],
    foldable: [],
    foldGeneration: 0,
    lines: { base: null, splices: [], chunks: [{ start: 0, lines }] },
    selections: [{ anchor: { line: 0, column: 0 }, head: { line: 0, column: 0 }, primary: true }],
    selectionCount: 1,
    primary: { position: { line: 0, column: 0 }, visualColumn: 0, row: 0 },
    dirty: false,
    search: null,
    highlights: [],
    diagnostics: [],
    language: null,
    decorations: [],
    changes: [],
    lenses: [],
    annotations: [],
    ...overrides,
  } as View
}

const hunk: Hunk = {
  index: 0,
  total: 2,
  original: [1, 2],
  modified: [1, 2],
  lines: { lineCount: 4, start: 1, lines: [{ text: 'was b', spans: [] }] },
  chars: [],
  applyTitle: 'Stage',
}

async function renderWithChanges(quickDiff = true) {
  const view = makeView(['a', 'B', 'c'], {
    changes: [
      { start: 1, end: 2, kind: 'modified' },
      { start: 3, end: 3, kind: 'removed' },
    ],
  })
  const transport: EditorTransport = {
    view: vi.fn(() => Promise.resolve(view)),
    apply: vi.fn(() => Promise.resolve(view)),
    clipboard: vi.fn(() => Promise.resolve(view)),
  }
  const { container } = render(
    <EditorView documentId={7} title="main.rs" transport={transport} quickDiff={quickDiff} />,
  )
  await act(async () => {})
  return container
}

describe('changes from the original', () => {
  it('are marked in the gutter', async () => {
    const container = await renderWithChanges()
    expect(container.querySelector('[data-change-line="1"]')).toHaveAttribute(
      'data-kind',
      'modified',
    )
    // Lines removed after the last line: below it.
    expect(container.querySelector('[data-change-line="2"]')).toHaveAttribute(
      'data-kind',
      'removedBelow',
    )
    expect(container.querySelector('[data-change-line="0"]')).toBeNull()
  })

  it("aren't marked where the editor doesn't show them (diff tabs)", async () => {
    const container = await renderWithChanges(false)
    expect(container.querySelector('[data-change-line]')).toBeNull()
  })

  it('open inline from their marker, to revert or stage', async () => {
    const user = userEvent.setup()
    vi.mocked(ipc.quickDiffHunk).mockResolvedValue(hunk)
    vi.mocked(ipc.quickDiffRevert).mockResolvedValue(true)
    vi.mocked(ipc.quickDiffApply).mockResolvedValue(true)
    const container = await renderWithChanges()

    fireEvent.mouseDown(container.querySelector('[data-change-line="1"]')!, { button: 0 })
    await act(async () => {})
    expect(ipc.quickDiffHunk).toHaveBeenCalledWith(7, 1, 'at')
    expect(screen.getByRole('region', { name: 'Change 1 of 2' })).toBeInTheDocument()
    expect(screen.getByText('was b')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Revert Change' }))
    expect(ipc.quickDiffRevert).toHaveBeenCalledWith(7, 0, [[1, 1]])
    expect(screen.queryByRole('region', { name: 'Change 1 of 2' })).toBeNull()

    fireEvent.mouseDown(container.querySelector('[data-change-line="1"]')!, { button: 0 })
    await act(async () => {})
    await user.click(screen.getByRole('button', { name: 'Stage Change' }))
    expect(ipc.quickDiffApply).toHaveBeenCalledWith(7, 0, [[1, 1]])
  })

  it('move to the next change, and close with Escape', async () => {
    const user = userEvent.setup()
    vi.mocked(ipc.quickDiffHunk)
      .mockResolvedValueOnce(hunk)
      .mockResolvedValueOnce({ ...hunk, index: 1, original: [3, 4], modified: [3, 3] })
    const container = await renderWithChanges()
    fireEvent.mouseDown(container.querySelector('[data-change-line="1"]')!, { button: 0 })
    await act(async () => {})
    await user.click(screen.getByRole('button', { name: 'Next Change' }))
    expect(ipc.quickDiffHunk).toHaveBeenLastCalledWith(7, 1, 'next')
    expect(await screen.findByRole('region', { name: 'Change 2 of 2' })).toBeInTheDocument()

    screen.getByRole('textbox', { name: 'Editor for main.rs' }).focus()
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('region', { name: 'Change 2 of 2' })).toBeNull()
  })
})
