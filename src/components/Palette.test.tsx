import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { registerCommands } from '../commands/registry'
import { activeEditor, type EditorController } from '../editor/activeEditor'
import { findFiles } from '../ipc/workspace'
import { iconThemeStore } from '../state/iconTheme'
import { workspaceStore } from '../state/workspace'
import { Palette } from './Palette'
import { openPalette, paletteStore } from './paletteState'

vi.mock('../ipc/workspace', async importOriginal => ({
  ...(await importOriginal<typeof import('../ipc/workspace')>()),
  findFiles: vi.fn(),
}))

describe('Palette', () => {
  let unregister = () => {}
  const save = vi.fn()
  const disabled = vi.fn()

  beforeEach(() => {
    unregister = registerCommands([
      { id: 'file.save', category: 'File', title: 'Save', keybinding: 'mod+s', run: save },
      {
        id: 'file.never',
        category: 'File',
        title: 'Never Shown',
        enabled: () => false,
        run: disabled,
      },
    ])
  })

  afterEach(() => {
    unregister()
    activeEditor.set(null)
    act(() => paletteStore.set({ open: false, initial: '', session: 0 }))
    vi.clearAllMocks()
  })

  it('lists enabled commands with their keybindings and runs the chosen one', async () => {
    const user = userEvent.setup()
    render(<Palette />)
    act(() => openPalette('>'))

    expect(screen.getByRole('option', { name: /File: Save/ })).toHaveTextContent('Ctrl+S')
    expect(screen.queryByRole('option', { name: /Never Shown/ })).not.toBeInTheDocument()

    await user.keyboard('sav{Enter}')
    expect(save).toHaveBeenCalledOnce()
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('goes to a line in the active editor', async () => {
    const user = userEvent.setup()
    const dispatch = vi.fn()
    const editor: EditorController = {
      documentId: 1,
      dispatch,
      openFind: vi.fn(),
      findNext: vi.fn(),
      refresh: vi.fn(),
      focus: vi.fn(),
      lineCount: () => 120,
      scrollToLine: vi.fn(),
      scrollBy: vi.fn(),
      goToDefinition: vi.fn(),
      triggerSuggest: vi.fn(),
      showHover: vi.fn(),
    }
    activeEditor.set(editor)
    render(<Palette />)
    act(() => openPalette(':'))

    await user.keyboard('42')
    expect(screen.getByRole('option')).toHaveTextContent('Go to line 42of 120')
    await user.keyboard('{Enter}')
    expect(dispatch).toHaveBeenCalledWith({
      type: 'setCursor',
      position: { line: 41, column: 0 },
      extend: false,
      add: false,
    })
  })

  it('closes on Escape', async () => {
    const user = userEvent.setup()
    render(<Palette />)
    act(() => openPalette('>'))
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('shows file results with their icons', async () => {
    vi.mocked(findFiles).mockResolvedValue({
      files: [
        { path: '/w/src/main.rs', relative: 'src/main.rs', indices: [4] },
        { path: '/w/README.md', relative: 'README.md', indices: [] },
      ],
      indexing: false,
      truncated: false,
    })
    const none = {
      file: null,
      folder: null,
      folderExpanded: null,
      fileExtensions: new Map<string, string>(),
      fileNames: new Map<string, string>(),
      folderNames: new Map<string, string>(),
      folderNamesExpanded: new Map<string, string>(),
    }
    workspaceStore.set({ info: { root: '/w', name: 'w' }, listings: new Map(), indexing: false })
    try {
      render(<Palette />)
      act(() => openPalette(''))
      const options = await screen.findAllByRole('option')
      expect(options.map(option => option.textContent)).toEqual(['main.rssrc', 'README.md'])
      const icons = options.map(option => option.querySelector('.palette__icon'))
      expect(icons.map(icon => icon?.tagName)).toEqual(['svg', 'svg'])

      act(() =>
        iconThemeStore.set({
          id: 'example.icons.colorful',
          icons: new Map([['rust', 'data:image/svg+xml;base64,cnM=']]),
          associations: { ...none, fileExtensions: new Map([['rs', 'rust']]) },
          light: none,
        }),
      )
      const rust = screen.getAllByRole('option')[0]?.querySelector('.palette__icon')
      expect(rust).toHaveAttribute('src', 'data:image/svg+xml;base64,cnM=')
      expect(rust).toHaveAttribute('alt', '')
    } finally {
      act(() => {
        iconThemeStore.set(null)
        workspaceStore.set({ info: null, listings: new Map(), indexing: false })
      })
    }
  })

  it('explains that files need an open folder', () => {
    render(<Palette />)
    act(() => openPalette(''))
    expect(screen.getByText('Open a folder to search its files')).toBeInTheDocument()
  })
})
