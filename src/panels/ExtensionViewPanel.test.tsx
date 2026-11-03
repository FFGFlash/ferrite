import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type {
  ExtensionStatus,
  MenuContribution,
  ViewContent,
  ViewContribution,
  ViewEntry,
} from '../ipc/extensions'

vi.mock('../ipc/extensions', () => ({
  getViewContent: vi.fn(),
  viewOpened: vi.fn(() => Promise.resolve()),
  runExtensionCommand: vi.fn(() => Promise.resolve()),
  runViewCommand: vi.fn(() => Promise.resolve()),
  expandViewItem: vi.fn(() => Promise.resolve()),
  setViewInput: vi.fn(() => Promise.resolve()),
}))

const ipc = await import('../ipc/extensions')
const { extensionsStore, viewChanged } = await import('../state/extensions')
const { contextMenuStore } = await import('../components/contextMenuState')
const { ExtensionContainerPanel, ExtensionViewActions, ExtensionViewPanel } =
  await import('./ExtensionViewPanel')

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
  icon: null,
  mark: null,
  context: null,
  expandable: false,
  ...overrides,
})

const content = (overrides: Partial<ViewContent>): ViewContent => ({
  welcome: null,
  badge: 0,
  flags: [],
  input: '',
  message: null,
  items: [],
  ...overrides,
})

const view = (id: string, overrides: Partial<ViewContribution> = {}): ViewContribution => ({
  id,
  fullId: `example.git.${id}`,
  title: id,
  icon: 'list',
  container: null,
  input: null,
  onExpand: null,
  ...overrides,
})

const menu = (command: string, overrides: Partial<MenuContribution>): MenuContribution => ({
  location: 'view/item',
  command: `example.git.${command}`,
  group: '',
  forFiles: true,
  forFolders: false,
  patterns: [],
  view: null,
  contexts: [],
  inline: false,
  icon: null,
  when: null,
  ...overrides,
})

/** An extension that contributes `views` and `menus` (commands named after them). */
function contribute(views: ViewContribution[], menus: MenuContribution[] = []): void {
  extensionsStore.set([
    {
      id: 'example.git',
      name: 'Git',
      state: 'active',
      commands: menus.map(item => ({
        id: item.command,
        title: item.command.split('.').pop() ?? item.command,
        category: 'Git',
        hidden: true,
      })),
      contributions: {
        menus,
        views,
        viewContainers: [
          { id: 'scm', fullId: 'example.git.scm', title: 'Source Control', icon: 'list' },
        ],
        colors: [],
      },
    } as unknown as ExtensionStatus,
  ])
}

describe('ExtensionViewPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    contribute([])
  })

  it("shows a tree, expands it, and runs an item's command with its argument", async () => {
    const user = userEvent.setup()
    vi.mocked(ipc.getViewContent).mockResolvedValue(
      content({
        message: '2 TODOs',
        items: [
          entry('main.rs', null, { expanded: true, description: '2' }),
          entry('todo-1', 'main.rs', { command: 'example.todos.reveal', argument: '1:4' }),
          entry('lib.rs', null),
          entry('todo-2', 'lib.rs'),
        ],
      }),
    )
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
    vi.mocked(ipc.getViewContent).mockResolvedValueOnce(content({}))
    render(<ExtensionViewPanel view="example.todos.reload" />)
    await waitFor(() => expect(ipc.getViewContent).toHaveBeenCalledTimes(1))
    vi.mocked(ipc.getViewContent).mockResolvedValueOnce(content({ message: 'Changed' }))
    act(() => viewChanged('example.todos.reload'))
    expect(await screen.findByText('Changed')).toBeInTheDocument()
  })

  it("runs items' buttons and menus for the selected items", async () => {
    const user = userEvent.setup()
    const changes = view('changes')
    contribute(
      [changes],
      [
        menu('stage', { view: changes.fullId, contexts: ['change'], inline: true, icon: 'add' }),
        menu('discard', { view: changes.fullId, contexts: ['change'] }),
        menu('refresh', {
          location: 'view/title',
          view: changes.fullId,
          inline: true,
          icon: 'refresh',
        }),
        menu('commitAll', { location: 'view/title', view: changes.fullId, when: 'repository' }),
        menu('init', { location: 'view/title', view: changes.fullId, when: '!repository' }),
      ],
    )
    const change = (name: string) =>
      entry(name, 'group', { context: 'change', argument: `/w/${name}` })
    vi.mocked(ipc.getViewContent).mockResolvedValue(
      content({
        flags: ['repository'],
        items: [
          entry('group', null, { expanded: true, context: 'group' }),
          change('a.rs'),
          change('b.rs'),
          change('c.rs'),
        ],
      }),
    )
    render(
      <>
        <ExtensionViewActions view={changes.fullId} />
        <ExtensionViewPanel view={changes.fullId} />
      </>,
    )
    const tree = await screen.findByRole('tree', { name: 'changes' })
    const row = (name: string) => within(tree).getByText(name).closest('li')!

    // One item's button: just that item.
    await user.click(within(row('a.rs')).getByRole('button', { name: 'stage' }))
    expect(ipc.runViewCommand).toHaveBeenLastCalledWith('example.git.stage', '/w/a.rs', ['/w/a.rs'])
    expect(within(row('group')).queryByRole('button', { name: 'stage' })).toBeNull()

    // Shift-click selects a range; a menu on one of them is for all of them.
    await user.click(row('a.rs'))
    await user.keyboard('{Shift>}')
    await user.click(row('c.rs'))
    await user.keyboard('{/Shift}')
    expect(row('b.rs')).toHaveAttribute('aria-selected', 'true')
    fireEvent.contextMenu(row('b.rs'))
    const entries = contextMenuStore.get()?.entries ?? []
    expect(entries.map(item => item?.label)).toEqual(['discard'])
    await act(async () => entries[0]?.run())
    expect(ipc.runViewCommand).toHaveBeenLastCalledWith('example.git.discard', '/w/b.rs', [
      '/w/a.rs',
      '/w/b.rs',
      '/w/c.rs',
    ])
    act(() => contextMenuStore.set(null))

    // The header: buttons, and the "…" menu with what its flags allow.
    await user.click(screen.getByRole('button', { name: 'refresh' }))
    expect(ipc.runViewCommand).toHaveBeenLastCalledWith('example.git.refresh', null, [])
    await user.click(screen.getByRole('button', { name: 'More Actions…' }))
    expect(contextMenuStore.get()?.entries.map(item => item?.label)).toEqual(['commitAll'])
    act(() => contextMenuStore.set(null))
  })

  it('asks for the children of an item opened without any', async () => {
    const user = userEvent.setup()
    const commits = view('commits', { onExpand: 'example.git.expand' })
    contribute([commits])
    vi.mocked(ipc.getViewContent).mockResolvedValue(
      content({ items: [entry('abc123', null, { expandable: true, label: 'Fix it' })] }),
    )
    let answer: () => void = () => {}
    vi.mocked(ipc.expandViewItem).mockImplementation(
      () => new Promise<void>(resolve => (answer = resolve)),
    )
    render(<ExtensionViewPanel view={commits.fullId} />)
    await user.click(await screen.findByText('Fix it'))
    expect(ipc.expandViewItem).toHaveBeenCalledWith(commits.fullId, 'abc123')
    expect(screen.getByText('Loading…')).toBeInTheDocument()

    // The extension sets the children, and the view changes.
    vi.mocked(ipc.getViewContent).mockResolvedValue(
      content({
        items: [
          entry('abc123', null, { expandable: true, label: 'Fix it' }),
          entry('abc123/main.rs', 'abc123', { label: 'main.rs' }),
        ],
      }),
    )
    await act(async () => {
      answer()
      viewChanged(commits.fullId)
    })
    expect(await screen.findByText('main.rs')).toBeInTheDocument()
    expect(screen.queryByText('Loading…')).toBeNull()
    expect(ipc.expandViewItem).toHaveBeenCalledTimes(1)
  })

  it('has a text box that runs its command, and a welcome message while empty', async () => {
    const user = userEvent.setup()
    const changes = view('box', {
      input: { placeholder: 'Message (Ctrl+Enter to commit)', command: 'example.git.commit' },
    })
    contribute([changes])
    // A new object each time, as from the backend.
    vi.mocked(ipc.getViewContent).mockImplementation(async () =>
      content({
        welcome: {
          text: 'Not a repository.\n\nInitialize one?',
          buttons: [
            { label: 'Initialize Repository', command: 'example.git.init', argument: null },
          ],
        },
      }),
    )
    render(<ExtensionViewPanel view={changes.fullId} />)
    const box = await screen.findByRole('textbox', { name: 'Message (Ctrl+Enter to commit)' })
    await user.type(box, 'Fix it')
    await user.keyboard('{Control>}{Enter}{/Control}')
    expect(ipc.setViewInput).toHaveBeenLastCalledWith(changes.fullId, 'Fix it')
    await waitFor(() =>
      expect(ipc.runExtensionCommand).toHaveBeenCalledWith('example.git.commit', null, null),
    )

    // The extension empties it after committing. The content it had last
    // was empty too (typing isn't sent back), and it's still emptied.
    act(() => viewChanged(changes.fullId))
    await waitFor(() => expect(box).toHaveValue(''))

    expect(screen.getByText('Initialize one?')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Initialize Repository' }))
    expect(ipc.runExtensionCommand).toHaveBeenLastCalledWith('example.git.init', null, null)
  })

  it("shows a container's views as sections that collapse, with their badges", async () => {
    const user = userEvent.setup()
    const changes = view('sectionA', { title: 'Changes', container: 'example.git.scm' })
    const commits = view('sectionB', { title: 'Commits', container: 'example.git.scm' })
    contribute([changes, commits])
    vi.mocked(ipc.getViewContent).mockImplementation(async id =>
      content(
        id === changes.fullId
          ? { badge: 3, items: [entry('x', null, { label: 'x.rs' })] }
          : { items: [entry('y', null, { label: 'First commit' })] },
      ),
    )
    render(<ExtensionContainerPanel container="example.git.scm" />)
    const toggle = await screen.findByRole('button', { name: 'Changes 3' })
    expect(toggle).toHaveAttribute('aria-expanded', 'true')
    expect(await screen.findByText('First commit')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Commits' }))
    expect(screen.queryByText('First commit')).toBeNull()
    expect(screen.getByText('x.rs')).toBeInTheDocument()
  })
})
