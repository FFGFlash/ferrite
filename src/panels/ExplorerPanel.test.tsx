import { act, fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { DirectoryEntry } from '../ipc/workspace'

vi.mock('../ipc/workspace', () => ({
  listDirectory: vi.fn(),
  createEntry: vi.fn(),
  getWorkspace: vi.fn(),
  openWorkspaceDialog: vi.fn(),
  closeWorkspace: vi.fn(),
}))
vi.mock('../ipc/editor', () => ({ openDocument: vi.fn() }))

const ipc = await import('../ipc/workspace')
const editorIpc = await import('../ipc/editor')
const { CommandError } = await import('../ipc/index')
const { setWorkspace, toggleFolder } = await import('../state/workspace')
const { documentsStore } = await import('../state/documents')
const { DEFAULT_LAYOUT, layoutStore } = await import('../state/layout')
const { notifications } = await import('../state/notifications')
const { iconThemeStore } = await import('../state/iconTheme')
const { themeKind } = await import('../themes/theme')
const { ExplorerActions, ExplorerPanel } = await import('./ExplorerPanel')
const { explorerStore } = await import('./explorerState')
const { contextMenuStore } = await import('../components/contextMenuState')
const { fileProblemsStore } = await import('../state/problems')

const entry = (path: string, directory = false, ignored = false): DirectoryEntry => ({
  name: path.split('/').pop() ?? path,
  path,
  directory,
  ignored,
})

let listings: Record<string, DirectoryEntry[]>

describe('ExplorerPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    layoutStore.set(DEFAULT_LAYOUT)
    explorerStore.set({ focused: null, creating: null, revealed: null })
    notifications.set([])
    listings = {
      '/w': [entry('/w/src', true), entry('/w/target', true, true), entry('/w/README.md')],
      '/w/src': [entry('/w/src/main.rs')],
    }
    vi.mocked(ipc.listDirectory).mockImplementation(async path => listings[path] ?? [])
    vi.mocked(ipc.createEntry).mockImplementation(async (parent, name, directory) => {
      const path = `${parent}/${name}`
      listings[parent] = [...(listings[parent] ?? []), entry(path, directory)]
      return { path, directories: [parent] }
    })
    vi.mocked(editorIpc.openDocument).mockImplementation(async path => ({
      id: 1,
      title: path.split('/').pop() ?? path,
      path,
      dirty: false,
      lineEnding: 'LF',
      encoding: 'UTF-8',
    }))
    documentsStore.set({ documents: [], activeId: null })
    fileProblemsStore.set({ files: [], leftOut: 0 })
  })

  async function renderTree() {
    render(
      <>
        <ExplorerActions />
        <ExplorerPanel />
      </>,
    )
    await act(async () => setWorkspace({ root: '/w', name: 'w' }))
    return screen.getByRole('tree', { name: 'w' })
  }

  it('offers to open a folder when none is open', () => {
    render(<ExplorerPanel />)
    act(() => setWorkspace(null))
    expect(screen.getByRole('button', { name: 'Open Folder…' })).toBeInTheDocument()
  })

  it('lists the root and marks ignored entries', async () => {
    const tree = await renderTree()
    const items = within(tree).getAllByRole('treeitem')
    expect(items.map(item => item.textContent)).toEqual(['▸src', '▸target', 'README.md'])
    expect(items[1]).toHaveAttribute('data-ignored')
  })

  it('marks everything inside an ignored folder as ignored', async () => {
    const user = userEvent.setup()
    // A rule can match inside an ignored folder without un-ignoring it.
    listings['/w/target'] = [entry('/w/target/debug', true), entry('/w/target/keep.txt')]
    const tree = await renderTree()
    await user.click(within(tree).getByText('target'))
    expect((await within(tree).findByText('keep.txt')).closest('li')).toHaveAttribute(
      'data-ignored',
    )
    expect(within(tree).getByText('debug').closest('li')).toHaveAttribute('data-ignored')
    expect(within(tree).getByText('src').closest('li')).not.toHaveAttribute('data-ignored')
  })

  it('marks files with problems and the folders that contain them', async () => {
    const user = userEvent.setup()
    const problem = (severity: 1 | 2 | 3) => ({
      start: { line: 0, column: 0 },
      end: { line: 0, column: 1 },
      severity,
      message: 'm',
      source: null,
    })
    fileProblemsStore.set({
      files: [
        {
          path: '/w/src/main.rs',
          document: null,
          title: 'main.rs',
          problems: [problem(1), problem(2)],
        },
        {
          path: '/w/README.md',
          document: 1,
          title: 'README.md',
          problems: [problem(2), problem(3)],
        },
      ],
      leftOut: 0,
    })
    const tree = await renderTree()
    const row = (name: string) => within(tree).getByText(name).closest('li')

    expect(row('src')).toHaveAttribute('data-problems', 'error')
    expect(row('src')).toHaveAttribute('aria-description', 'Contains 1 error, 1 warning')
    expect(row('target')).not.toHaveAttribute('data-problems')
    expect(row('README.md')).toHaveAttribute('data-problems', 'warning')
    expect(row('README.md')).toHaveAttribute('aria-description', '1 warning')
    expect(row('README.md')).toHaveAttribute('title', '/w/README.md\n1 warning')
    expect(row('README.md')?.querySelector('.explorer-tree__problems')).toHaveTextContent('1')

    await user.click(within(tree).getByText('src'))
    await within(tree).findByText('main.rs')
    expect(row('main.rs')).toHaveAttribute('data-problems', 'error')
    expect(row('main.rs')?.querySelector('.explorer-tree__problems')).toHaveTextContent('2')

    // Fixed: the marks go.
    act(() => fileProblemsStore.set({ files: [], leftOut: 0 }))
    expect(row('src')).not.toHaveAttribute('data-problems')
    expect(row('main.rs')?.querySelector('.explorer-tree__problems')).toBeNull()
  })

  it('expands folders lazily and opens files', async () => {
    const user = userEvent.setup()
    const tree = await renderTree()
    await user.click(within(tree).getByText('src'))
    expect(await within(tree).findByText('main.rs')).toBeInTheDocument()
    expect(ipc.listDirectory).toHaveBeenCalledWith('/w/src')
    expect(layoutStore.get().expanded).toEqual(['/w/src'])

    await user.click(within(tree).getByText('README.md'))
    expect(editorIpc.openDocument).toHaveBeenCalledWith('/w/README.md')
  })

  it('navigates with the keyboard', async () => {
    const user = userEvent.setup()
    const tree = await renderTree()
    tree.focus()
    await user.keyboard('{ArrowRight}')
    expect(await within(tree).findByText('main.rs')).toBeInTheDocument()
    await user.keyboard('{ArrowDown}')
    expect(within(tree).getByText('main.rs').closest('li')).toHaveAttribute('data-focused')
    await user.keyboard('{ArrowLeft}')
    expect(within(tree).getByText('src').closest('li')).toHaveAttribute('data-focused')
    await user.keyboard('{ArrowLeft}')
    expect(within(tree).queryByText('main.rs')).not.toBeInTheDocument()
  })

  it("opens the focused file's menu from the keyboard", async () => {
    const user = userEvent.setup()
    const tree = await renderTree()
    tree.focus()
    await user.keyboard('{End}')
    fireEvent.contextMenu(tree)
    expect(contextMenuStore.get()?.entries[0]).toMatchObject({ label: 'Open' })
  })

  it('reveals the active file: expands its folders, selects it, and scrolls to it', async () => {
    const scrolled = vi.spyOn(Element.prototype, 'scrollIntoView')
    const tree = await renderTree()
    expect(within(tree).queryByText('main.rs')).not.toBeInTheDocument()

    // As when Go to Definition opens a file in a collapsed folder.
    const summary = {
      id: 3,
      title: 'main.rs',
      path: '/w/src/main.rs',
      dirty: false,
      lineEnding: 'LF',
      encoding: 'UTF-8',
    }
    await act(async () => documentsStore.set({ documents: [summary], activeId: 3 }))
    const row = (await within(tree).findByText('main.rs')).closest('li')
    expect(layoutStore.get().expanded).toEqual(['/w/src'])
    expect(row).toHaveAttribute('aria-selected', 'true')
    expect(row).toHaveAttribute('data-focused')
    expect(scrolled.mock.contexts).toContain(row)

    // Collapsing it afterwards sticks: only a change of file reveals again.
    scrolled.mockClear()
    act(() => toggleFolder('/w/src'))
    await act(async () =>
      documentsStore.update(state => ({ ...state, documents: [{ ...summary, dirty: true }] })),
    )
    expect(layoutStore.get().expanded).toEqual([])
    expect(scrolled).not.toHaveBeenCalled()
    act(() => documentsStore.set({ documents: [], activeId: null }))
    scrolled.mockRestore()
  })

  it('shows folder and file icons', async () => {
    const tree = await renderTree()
    const kinds = within(tree)
      .getAllByRole('treeitem')
      .map(item => item.querySelector('svg')?.dataset.kind)
    expect(kinds).toEqual(['folder', 'folder', 'file'])
  })

  it("shows the icon theme's images where it has them", async () => {
    const none = {
      file: null,
      folder: null,
      folderExpanded: null,
      fileExtensions: new Map<string, string>(),
      fileNames: new Map<string, string>(),
      folderNames: new Map<string, string>(),
      folderNamesExpanded: new Map<string, string>(),
    }
    iconThemeStore.set({
      id: 'example.icons.colorful',
      icons: new Map([
        ['src', 'data:image/svg+xml;base64,c3Jj'],
        ['md', 'data:image/svg+xml;base64,bWQ='],
      ]),
      associations: { ...none, folderNames: new Map([['src', 'src']]) },
      light: { ...none, fileExtensions: new Map([['md', 'md']]) },
    })
    try {
      const tree = await renderTree()
      const icons = within(tree)
        .getAllByRole('treeitem')
        .map(item => item.querySelector('.explorer-tree__icon'))
      expect(icons[0]).toHaveAttribute('src', 'data:image/svg+xml;base64,c3Jj')
      expect(icons[0]).toHaveAttribute('alt', '')
      expect(icons[1]?.tagName).toBe('svg')
      // The light section only applies with a light color theme.
      expect(icons[2]?.tagName).toBe('svg')
      act(() => themeKind.set('light'))
      expect(within(tree).getByText('README.md').previousElementSibling).toHaveAttribute(
        'src',
        'data:image/svg+xml;base64,bWQ=',
      )
    } finally {
      act(() => {
        iconThemeStore.set(null)
        themeKind.set('dark')
      })
    }
  })

  it('creates a file at the root from the header and opens it', async () => {
    const user = userEvent.setup()
    const tree = await renderTree()
    await user.click(screen.getByRole('button', { name: 'New File' }))
    const input = within(tree).getByRole('textbox', { name: 'File name' })
    expect(input).toHaveFocus()
    expect(input.closest('li')).toHaveAttribute('aria-level', '1')

    await user.type(input, 'notes.md{Enter}')
    expect(ipc.createEntry).toHaveBeenCalledWith('/w', 'notes.md', false)
    expect(await within(tree).findByText('notes.md')).toBeInTheDocument()
    expect(within(tree).queryByRole('textbox')).not.toBeInTheDocument()
    expect(within(tree).getByText('notes.md').closest('li')).toHaveAttribute('data-focused')
    expect(editorIpc.openDocument).toHaveBeenCalledWith('/w/notes.md')
  })

  it("creates a folder inside a folder from the folder's menu", async () => {
    const user = userEvent.setup()
    const tree = await renderTree()
    fireEvent.contextMenu(within(tree).getByText('src'))
    const menu = contextMenuStore.get()
    expect(menu?.entries.map(item => item?.label)).toEqual(['New File…', 'New Folder…'])
    await act(async () => menu?.entries[1]?.run())

    expect(layoutStore.get().expanded).toEqual(['/w/src'])
    const input = await within(tree).findByRole('textbox', { name: 'Folder name' })
    expect(input.closest('li')).toHaveAttribute('aria-level', '2')
    await user.type(input, 'net{Enter}')
    expect(ipc.createEntry).toHaveBeenCalledWith('/w/src', 'net', true)
    expect(await within(tree).findByText('net')).toBeInTheDocument()
    expect(tree).toHaveFocus()
    expect(editorIpc.openDocument).not.toHaveBeenCalled()
  })

  it("creates beside the focused file and offers creation in a file's menu", async () => {
    const user = userEvent.setup()
    const tree = await renderTree()
    await user.click(within(tree).getByText('src'))
    fireEvent.contextMenu(await within(tree).findByText('main.rs'))
    expect(contextMenuStore.get()?.entries.map(item => item?.label ?? null)).toEqual([
      'Open',
      null,
      'New File…',
      'New Folder…',
    ])
    act(() => contextMenuStore.set(null))

    await user.click(screen.getByRole('button', { name: 'New File' }))
    await user.type(within(tree).getByRole('textbox'), 'lib.rs{Enter}')
    expect(ipc.createEntry).toHaveBeenCalledWith('/w/src', 'lib.rs', false)
  })

  it('creates at the top level from the space below the rows', async () => {
    const user = userEvent.setup()
    const tree = await renderTree()
    await user.click(within(tree).getByText('src'))
    await user.click(await within(tree).findByText('main.rs'))
    const blank = document.querySelector('.explorer__blank')
    expect(blank).not.toBeNull()
    fireEvent.contextMenu(blank as Element)
    const menu = contextMenuStore.get()
    expect(menu?.entries.map(item => item?.label)).toEqual(['New File…', 'New Folder…'])
    await act(async () => menu?.entries[0]?.run())

    const input = within(tree).getByRole('textbox', { name: 'File name' })
    expect(input.closest('li')).toHaveAttribute('aria-level', '1')
    await user.type(input, 'top.txt{Enter}')
    expect(ipc.createEntry).toHaveBeenCalledWith('/w', 'top.txt', false)
  })

  it('keeps the field open with the problem when creating fails', async () => {
    const user = userEvent.setup()
    const tree = await renderTree()
    vi.mocked(ipc.createEntry).mockRejectedValueOnce(
      new CommandError('invalidName', `".." can't be used as a name.`),
    )
    await user.click(screen.getByRole('button', { name: 'New Folder' }))
    const input = within(tree).getByRole('textbox')
    await user.type(input, '..{Enter}')
    expect(await within(tree).findByRole('alert')).toHaveTextContent("can't be used as a name")
    expect(input).toHaveAttribute('aria-invalid', 'true')
    expect(input).toHaveFocus()

    await user.keyboard('{Escape}')
    expect(within(tree).queryByRole('textbox')).not.toBeInTheDocument()
    expect(tree).toHaveFocus()
    expect(notifications.get()).toEqual([])
  })

  it('warns about a name that is taken and cancels when left', async () => {
    const user = userEvent.setup()
    const tree = await renderTree()
    await user.click(screen.getByRole('button', { name: 'New File' }))
    await user.type(within(tree).getByRole('textbox'), 'readme.md')
    expect(within(tree).getByRole('alert')).toHaveTextContent(
      'A file or folder named README.md already exists here.',
    )
    await user.click(document.body)
    expect(within(tree).queryByRole('textbox')).not.toBeInTheDocument()
    expect(ipc.createEntry).not.toHaveBeenCalled()
  })

  it('creates the entry when the field is left with a new name', async () => {
    const user = userEvent.setup()
    const tree = await renderTree()
    await user.click(screen.getByRole('button', { name: 'New File' }))
    await user.type(within(tree).getByRole('textbox'), 'todo.txt')
    await user.click(document.body)
    expect(ipc.createEntry).toHaveBeenCalledWith('/w', 'todo.txt', false)
    expect(await within(tree).findByText('todo.txt')).toBeInTheDocument()
  })
})
