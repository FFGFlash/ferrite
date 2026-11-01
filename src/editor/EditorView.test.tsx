import { act, fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { EditorOp, Viewport, WireView as View } from '../ipc/editor'
import { editorStatus } from '../state/editorStatus'
import { contextMenuStore } from '../components/contextMenuState'
import { EditorView } from './EditorView'
import type { EditorTransport } from './session'

/** A full (non-incremental) view of `lines`. */
function makeView(lines: string[], overrides: Partial<View> = {}): View {
  return {
    documentId: 7,
    version: 1,
    lineCount: lines.length,
    startLine: 0,
    endLine: lines.length,
    lines: { base: null, splices: [], chunks: [{ start: 0, lines }] },
    selections: [{ anchor: { line: 0, column: 0 }, head: { line: 0, column: 0 }, primary: true }],
    selectionCount: 1,
    primary: { position: { line: 0, column: 0 }, visualColumn: 0 },
    dirty: false,
    search: null,
    highlights: [],
    diagnostics: [],
    language: null,
    decorations: [],
    ...overrides,
  } as View
}

function fakeTransport(initial: View) {
  const applied: EditorOp[][] = []
  const transport: EditorTransport = {
    view: vi.fn((_id: number, _viewport: Viewport) => Promise.resolve(initial)),
    apply: vi.fn((_id: number, ops: EditorOp[]) => {
      applied.push(ops)
      return Promise.resolve(
        makeView(['changed'], {
          version: 2,
          dirty: true,
          primary: { position: { line: 0, column: 3 }, visualColumn: 3 },
        }),
      )
    }),
    clipboard: vi.fn(() => Promise.resolve(initial)),
  }
  return { transport, applied }
}

async function renderEditor(transport: EditorTransport) {
  render(<EditorView documentId={7} title="main.rs" transport={transport} />)
  await act(async () => {})
  return screen.getByRole('textbox', { name: 'Editor for main.rs' })
}

describe('EditorView', () => {
  afterEach(() => {
    editorStatus.set(null)
  })

  it('renders the lines of the initial view with line numbers', async () => {
    const { transport } = fakeTransport(makeView(['fn main() {', '}']))
    await renderEditor(transport)
    expect(screen.getByText('fn main() {')).toBeInTheDocument()
    expect(screen.getByText('2')).toBeInTheDocument()
    expect(transport.view).toHaveBeenCalledWith(
      7,
      expect.objectContaining({ startLine: 0 }),
      null,
      null,
    )
  })

  it('sends typed text and keys as operations and renders the result', async () => {
    const user = userEvent.setup()
    const { transport, applied } = fakeTransport(makeView(['']))
    const input = await renderEditor(transport)

    input.focus()
    await user.keyboard('x')
    expect(applied[0]).toEqual([{ type: 'insert', text: 'x' }])
    expect(await screen.findByText('changed')).toBeInTheDocument()

    await user.keyboard('{Backspace}')
    await user.keyboard('{Shift>}{ArrowRight}{/Shift}')
    expect(applied.flat()).toContainEqual({
      type: 'delete',
      direction: 'backward',
      unit: 'grapheme',
    })
    expect(applied.flat()).toContainEqual({ type: 'move', motion: 'right', extend: true })
  })

  it('reports the cursor position for the status bar', async () => {
    const user = userEvent.setup()
    const { transport } = fakeTransport(makeView(['abc']))
    const input = await renderEditor(transport)
    expect(editorStatus.get()).toEqual({ line: 1, column: 1, selectionCount: 1, language: null })
    input.focus()
    await user.keyboard('a')
    expect(editorStatus.get()).toEqual({ line: 1, column: 4, selectionCount: 1, language: null })
  })

  it("holds the cursor's line and selection for screen readers", async () => {
    const user = userEvent.setup()
    const { transport } = fakeTransport(
      makeView(['fn main() {', '  ok();', '}'], {
        selections: [
          { anchor: { line: 1, column: 2 }, head: { line: 1, column: 6 }, primary: true },
        ],
        primary: { position: { line: 1, column: 6 }, visualColumn: 6 },
      }),
    )
    const input = (await renderEditor(transport)) as HTMLTextAreaElement
    expect(input.value).toBe('  ok();')
    expect([input.selectionStart, input.selectionEnd]).toEqual([2, 6])

    // After typing, it follows the view the backend returns.
    input.focus()
    await user.keyboard('x')
    expect(await screen.findByText('changed')).toBeInTheDocument()
    expect(input.value).toBe('changed')
    expect(input.selectionStart).toBe(0)
  })

  it('inserts composed text once, when composition ends', async () => {
    const { transport, applied } = fakeTransport(makeView(['ab']))
    const input = (await renderEditor(transport)) as HTMLTextAreaElement
    fireEvent.compositionStart(input)
    input.value = 'かab'
    fireEvent.input(input, { isComposing: true })
    expect(applied).toHaveLength(0)
    fireEvent.compositionEnd(input, { data: 'か' })
    fireEvent.input(input)
    expect(applied).toEqual([[{ type: 'insert', text: 'か' }]])
    expect(input.value).toBe('ab')
  })

  it('opens the context menu from the keyboard without moving the cursor', async () => {
    const { transport, applied } = fakeTransport(makeView(['abc']))
    const input = await renderEditor(transport)
    fireEvent.contextMenu(input)
    expect(contextMenuStore.get()?.entries.map(entry => entry?.label)).toContain('Copy')
    expect(applied).toHaveLength(0)
    contextMenuStore.set(null)
  })

  it('moves the cursor to the end of the last line when clicked below it', async () => {
    const { transport, applied } = fakeTransport(makeView(['fn main() {', '}']))
    const input = await renderEditor(transport)
    const content = document.querySelector('.editor-view__content')
    expect(content).not.toBeNull()
    // Lines start at y = 0 in jsdom, so y = 500 is far below the second line.
    fireEvent.mouseDown(content as Element, { button: 0, clientX: 40, clientY: 500 })
    fireEvent.mouseUp(window)
    await act(async () => {})
    expect(input).toHaveFocus()
    expect(applied.flat()).toContainEqual({
      type: 'setCursor',
      position: { line: 1, column: Number.MAX_SAFE_INTEGER },
      extend: false,
      add: false,
    })
  })

  it('sends pasted text as a paste', async () => {
    const user = userEvent.setup()
    const { transport, applied } = fakeTransport(makeView(['']))
    const input = await renderEditor(transport)
    input.focus()
    await user.paste('pasted\ntext')
    expect(applied[0]).toEqual([{ type: 'paste', text: 'pasted\ntext' }])
  })
})
