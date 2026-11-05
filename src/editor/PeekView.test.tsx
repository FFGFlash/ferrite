import { act, fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { WireView as View } from '../ipc/editor'
import * as editorIpc from '../ipc/editor'
import * as lsp from '../ipc/lsp'
import { documentsStore } from '../state/documents'
import { notifications } from '../state/notifications'
import { editorFor } from './activeEditor'
import { EditorView } from './EditorView'
import { peekStore } from './references'
import type { EditorTransport } from './session'

vi.mock('../ipc/lsp', () => ({
  references: vi.fn(),
  definition: vi.fn(),
  hover: vi.fn(),
  completion: vi.fn(),
}))
vi.mock('../ipc/editor', async importOriginal => ({
  ...(await importOriginal<typeof import('../ipc/editor')>()),
  openDocument: vi.fn(),
  peekLines: vi.fn(),
}))

const LINES = ['fn total() -> u32 {', '  1', '}', '', 'fn main() {', '  total();', '}']

function makeView(): View {
  return {
    documentId: 7,
    version: 1,
    lineCount: LINES.length,
    rowCount: LINES.length,
    startRow: 0,
    startLine: 0,
    endLine: LINES.length,
    visible: [[0, LINES.length]],
    folded: [],
    foldable: [],
    foldGeneration: 0,
    lines: { base: null, splices: [], chunks: [{ start: 0, lines: LINES }] },
    selections: [{ anchor: { line: 5, column: 2 }, head: { line: 5, column: 2 }, primary: true }],
    selectionCount: 1,
    primary: { position: { line: 5, column: 2 }, visualColumn: 2, row: 5 },
    dirty: false,
    search: null,
    highlights: [],
    diagnostics: [],
    language: null,
    decorations: [],
    changes: [],
    lenses: [],
  } as View
}

const transport: EditorTransport = {
  view: () => Promise.resolve(makeView()),
  apply: () => Promise.resolve(makeView()),
  clipboard: () => Promise.resolve(makeView()),
}

const reference = (path: string, line: number, column: number): lsp.Reference => ({
  path,
  start: { line, column },
  end: { line, column: column + 5 },
  line: path === '/w/main.rs' ? LINES[line]! : 'use crate::total;',
})

async function renderEditor() {
  documentsStore.set({
    documents: [
      {
        id: 7,
        title: 'main.rs',
        path: '/w/main.rs',
        dirty: false,
        lineEnding: 'LF',
        encoding: 'UTF-8',
        encodingId: 'utf-8',
      },
    ],
    activeId: 7,
  })
  vi.mocked(editorIpc.peekLines).mockImplementation(async (_path, start) => ({
    lineCount: LINES.length,
    start,
    lines: LINES.slice(start).map(text => ({ text, spans: [] })),
  }))
  render(<EditorView documentId={7} title="main.rs" transport={transport} />)
  await act(async () => {})
}

describe('the references peek', () => {
  afterEach(() => {
    act(() => peekStore.set(null))
    documentsStore.set({ documents: [], activeId: null })
    vi.clearAllMocks()
  })

  it('peeks at several references, starting on the nearest, and closes with Escape', async () => {
    vi.mocked(lsp.references).mockResolvedValue([
      reference('/w/lib.rs', 0, 11),
      reference('/w/main.rs', 0, 3),
      reference('/w/main.rs', 5, 2),
    ])
    await renderEditor()
    await act(async () => editorFor(7)!.goToReferences())

    const peek = screen.getByRole('region', { name: 'References: main.rs' })
    expect(within(peek).getByText('References (3)')).toBeInTheDocument()
    const list = within(peek).getByRole('tree', { name: 'References' })
    expect(list).toHaveFocus()
    // Files other than the one looked at start collapsed.
    const files = within(list)
      .getAllByRole('treeitem')
      .filter(item => item.hasAttribute('aria-expanded'))
    expect(files.map(file => file.getAttribute('aria-expanded'))).toEqual(['false', 'true'])
    expect(within(list).getByRole('treeitem', { selected: true })).toHaveTextContent('total();')

    // Moving up the list shows the reference above in the preview.
    fireEvent.keyDown(list, { key: 'ArrowUp' })
    expect(peekStore.get()?.selected).toEqual({ group: 1, index: 0 })
    await act(async () => {})
    expect(editorIpc.peekLines).toHaveBeenLastCalledWith('/w/main.rs', 0, expect.any(Number))

    fireEvent.keyDown(list, { key: 'Escape' })
    expect(screen.queryByRole('region', { name: /References/ })).not.toBeInTheDocument()
    expect(screen.getByRole('textbox', { name: 'Editor for main.rs' })).toHaveFocus()
  })

  it('goes straight to a lone reference, but Peek References still peeks', async () => {
    const only = reference('/w/main.rs', 0, 3)
    vi.mocked(lsp.references).mockResolvedValue([only])
    vi.mocked(editorIpc.openDocument).mockResolvedValue(documentsStore.get().documents[0]!)
    await renderEditor()

    await act(async () => editorFor(7)!.goToReferences())
    expect(screen.queryByRole('region', { name: /References/ })).not.toBeInTheDocument()
    expect(editorIpc.openDocument).toHaveBeenCalledWith('/w/main.rs', false)

    await act(async () => editorFor(7)!.peekReferences())
    expect(screen.getByRole('region', { name: 'References: main.rs' })).toBeInTheDocument()
  })

  it('names the symbol when there are no references', async () => {
    vi.mocked(lsp.references).mockResolvedValue([])
    await renderEditor()
    await act(async () => editorFor(7)!.goToReferences())
    expect(JSON.stringify(notifications.get())).toContain("No references found for 'total'.")
  })
})
