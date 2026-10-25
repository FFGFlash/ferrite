import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { registerCommands } from '../commands/registry'
import { openContextMenu } from '../components/contextMenuState'
import { openPalette, paletteStore } from '../components/paletteState'
import { Breadcrumbs } from '../editor/Breadcrumbs'
import { EditorView } from '../editor/EditorView'
import type { EditorTransport } from '../editor/session'
import type { WireView } from '../ipc/editor'
import { extensionsStore } from '../state/extensions'
import { DEFAULT_LAYOUT, layoutStore } from '../state/layout'
import { notifications, notify } from '../state/notifications'
import { workspaceStore } from '../state/workspace'
import { explorerStore } from '../panels/explorerState'
import { axeViolations } from '../test/axe'
import { Workbench } from './Workbench'

const view = {
  documentId: 1,
  version: 1,
  lineCount: 2,
  startLine: 0,
  endLine: 2,
  lines: { base: null, splices: [], chunks: [{ start: 0, lines: ['fn main() {', '}'] }] },
  selections: [{ anchor: { line: 0, column: 3 }, head: { line: 0, column: 3 }, primary: true }],
  selectionCount: 1,
  primary: { position: { line: 0, column: 3 }, visualColumn: 3 },
  dirty: false,
  search: null,
  highlights: [],
  diagnostics: [],
  language: null,
  decorations: [],
} as WireView

describe('accessibility (axe)', () => {
  beforeEach(() => {
    layoutStore.set(DEFAULT_LAYOUT)
    notifications.set([])
  })

  afterEach(() => {
    act(() => paletteStore.set({ open: false, initial: '', session: 0 }))
  })

  it('finds no problems in the workbench and its views', async () => {
    const user = userEvent.setup()
    extensionsStore.set([
      {
        id: 'example.tidy',
        name: 'Tidy',
        version: '0.1.0',
        description: 'Flags trailing whitespace.',
        path: '/ext/tidy',
        development: true,
        state: 'failed',
        error: 'The command took longer than 10 s and was stopped',
        permissions: [
          {
            permission: 'documents-read',
            description: 'Read open documents',
            granted: true,
            decided: true,
          },
        ],
        commands: [{ id: 'example.tidy.trim', title: 'Trim', category: 'Tidy', hidden: false }],
        configErrors: ['max_line_length: expected an integer'],
        contributions: {
          themes: [],
          iconThemes: [],
          keybindings: [],
          menus: [],
          views: [],
          decorations: [],
        },
        usage: {
          busyMs: 12,
          calls: 3,
          slowestCallMs: 10,
          memoryBytes: 0,
          peakMemoryBytes: 1 << 20,
          droppedEvents: 0,
          compileMs: 40,
        },
      },
    ])
    render(<Workbench appInfo={null} />)
    expect(await axeViolations()).toEqual([])

    for (const name of ['Search', 'Extensions']) {
      await user.click(screen.getByRole('button', { name }))
      expect(await axeViolations(), name).toEqual([])
    }
  })

  it('finds no problems in the explorer while naming a new file', async () => {
    const user = userEvent.setup()
    const entry = (name: string, directory: boolean) => ({
      name,
      path: `/w/${name}`,
      directory,
      ignored: false,
    })
    workspaceStore.set({
      info: { root: '/w', name: 'w' },
      listings: new Map([['/w', [entry('src', true), entry('README.md', false)]]]),
      indexing: false,
    })
    explorerStore.set({
      focused: null,
      creating: { parent: '/w', directory: false },
      revealed: null,
    })
    render(<Workbench appInfo={null} />)
    await user.type(screen.getByRole('textbox', { name: 'File name' }), 'readme.md')
    expect(screen.getByRole('alert')).toBeInTheDocument()
    expect(await axeViolations()).toEqual([])
    act(() => {
      explorerStore.set({ focused: null, creating: null, revealed: null })
      workspaceStore.set({ info: null, listings: new Map(), indexing: false })
    })
  })

  it('finds no problems in the palette, menus, and notifications', async () => {
    const unregister = registerCommands([
      { id: 'file.save', category: 'File', title: 'Save', keybinding: 'mod+s', run: () => {} },
    ])
    render(<Workbench appInfo={null} />)
    act(() => {
      openPalette('>')
      notify('warning', 'Unsaved changes were restored.', [{ label: 'Show', run: () => {} }])
      notify('error', 'Saving failed.')
    })
    expect(await axeViolations()).toEqual([])
    act(() => paletteStore.set({ open: false, initial: '', session: 0 }))

    act(() =>
      openContextMenu(10, 10, [
        { label: 'Cut', run: () => {} },
        null,
        { label: 'Copy', run: () => {} },
      ]),
    )
    // A popup menu belongs to no landmark (a best practice, not a WCAG rule).
    const menu = screen.getByRole('menu')
    expect(await axeViolations(menu, ['region'])).toEqual([])
    unregister()
  })

  it('finds no problems in the editor and its breadcrumbs', async () => {
    const transport: EditorTransport = {
      view: vi.fn(() => Promise.resolve(view)),
      apply: vi.fn(() => Promise.resolve(view)),
      clipboard: vi.fn(() => Promise.resolve(view)),
    }
    workspaceStore.set({ info: { root: '/w', name: 'w' }, listings: new Map(), indexing: false })
    const document = {
      id: 1,
      title: 'main.rs',
      path: '/w/src/main.rs',
      dirty: false,
      lineEnding: 'LF',
      encoding: 'UTF-8',
    }
    render(
      <main>
        <Breadcrumbs document={document} />
        <EditorView documentId={1} title="main.rs" transport={transport} />
      </main>,
    )
    await act(async () => {})
    expect(await axeViolations()).toEqual([])
    act(() => workspaceStore.set({ info: null, listings: new Map(), indexing: false }))
  })
})
