import { act, fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ExtensionStatus } from '../ipc/extensions'

vi.mock('../ipc/extensions', () => ({
  restartExtension: vi.fn(() => Promise.resolve()),
  setExtensionEnabled: vi.fn(() => Promise.resolve()),
  setExtensionPermissions: vi.fn(() => Promise.resolve()),
  getExtensionIcon: vi.fn(() => Promise.resolve('data:image/svg+xml;base64,PHN2Zy8+')),
  getExtensionReadme: vi.fn(),
  getExtensionReadmeImage: vi.fn(),
  listExtensions: vi.fn(() => new Promise(() => {})),
}))

const ipc = await import('../ipc/extensions')
const { extensionsStore } = await import('../state/extensions')
const { setPageHandler } = await import('../state/viewRequests')
const { contextMenuStore } = await import('../components/contextMenuState')
const { ExtensionsPanel, rowTooltip } = await import('./ExtensionsPanel')
const { ExtensionPage } = await import('./ExtensionPage')

const tidy: ExtensionStatus = {
  id: 'example.tidy',
  name: 'Tidy',
  version: '0.1.0',
  description: 'Flags trailing whitespace and overlong lines, and trims trailing whitespace.',
  author: 'Ferrite',
  hasIcon: true,
  hasReadme: true,
  path: '/ext/tidy',
  development: true,
  state: 'active',
  error: null,
  permissions: [
    {
      permission: 'documents-read',
      description: 'Read open documents',
      granted: true,
      decided: true,
    },
    {
      permission: 'documents-write',
      description: 'Edit open documents',
      granted: false,
      decided: true,
    },
  ],
  commands: [{ id: 'example.tidy.trim', title: 'Trim', category: 'Tidy', hidden: false }],
  configErrors: ['width must be at least 0; using the default'],
  hasSettings: false,
  contributions: {
    themes: [{ id: 'example.tidy.night', label: 'Night', kind: 'dark', colors: {} }],
    iconThemes: [],
    keybindings: [],
    menus: [],
    views: [],
    decorations: [],
    previews: [],
    languages: [],
    languageServers: [],
  },
  usage: {
    busyMs: 3.42,
    calls: 12,
    slowestCallMs: 1.2,
    memoryBytes: 1153434,
    peakMemoryBytes: 0,
    droppedEvents: 0,
    compileMs: 30,
    activationMs: 42.4,
  },
}

const broken: ExtensionStatus = {
  ...tidy,
  id: '/ext/broken',
  name: 'broken',
  version: '',
  description: '',
  author: null,
  hasIcon: false,
  hasReadme: false,
  state: 'invalid',
  error: 'requires extension API version 2',
  permissions: [],
  commands: [],
  configErrors: [],
  development: false,
  usage: { ...tidy.usage, calls: 0, activationMs: null },
}

describe('ExtensionsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    contextMenuStore.set(null)
  })

  it('explains how to add extensions when there are none', () => {
    extensionsStore.set([])
    render(<ExtensionsPanel />)
    expect(screen.getByText('No extensions are installed.')).toBeInTheDocument()
  })

  it('shows one row per extension: icon, name, description, author, and start time', async () => {
    extensionsStore.set([tidy, broken])
    render(<ExtensionsPanel />)
    const [row, other] = screen.getAllByRole('listitem')
    const open = within(row!).getByRole('button', { name: 'Tidy' })
    expect(open).toHaveTextContent('Tidy')
    expect(open).toHaveTextContent('dev')
    expect(open).toHaveTextContent('42 ms')
    expect(open).toHaveTextContent('Ferrite')
    expect(open).toHaveAttribute('aria-description', tidy.description)
    // The tooltip has what the row shortens.
    expect(open.title).toBe(
      [
        'Tidy 0.1.0 (development)',
        'example.tidy · by Ferrite',
        tidy.description,
        'Active · started in 42 ms',
      ].join('\n'),
    )
    expect(await within(row!).findByRole('presentation')).toHaveAttribute(
      'src',
      'data:image/svg+xml;base64,PHN2Zy8+',
    )
    // A broken extension says so, and has no icon to load.
    expect(within(other!).getByRole('button', { name: 'broken, Invalid' })).toHaveTextContent(
      'requires extension API version 2',
    )
    expect(ipc.getExtensionIcon).toHaveBeenCalledTimes(1)
  })

  it('opens the page of the extension chosen, and moves between rows with the arrow keys', async () => {
    const user = userEvent.setup()
    const shown: string[] = []
    setPageHandler(id => shown.push(id))
    extensionsStore.set([tidy, broken])
    render(<ExtensionsPanel />)
    await user.click(screen.getByRole('button', { name: 'Tidy' }))
    expect(shown).toEqual(['extension:example.tidy'])
    await user.keyboard('{ArrowDown}')
    expect(screen.getByRole('button', { name: 'broken, Invalid' })).toHaveFocus()
  })

  it('offers the actions in a manage menu', async () => {
    const user = userEvent.setup()
    extensionsStore.set([tidy])
    render(<ExtensionsPanel />)
    await user.click(screen.getByRole('button', { name: 'Manage Tidy' }))
    const entries = contextMenuStore.get()?.entries ?? []
    expect(entries.map(entry => entry?.label ?? null)).toEqual([
      'Show Details',
      null,
      'Disable',
      'Restart',
      null,
      'Show Log',
      null,
      'Copy Extension ID',
    ])
    await act(async () => entries.find(entry => entry?.label === 'Disable')?.run())
    expect(ipc.setExtensionEnabled).toHaveBeenCalledWith('example.tidy', false)

    act(() => contextMenuStore.set(null))
    fireEvent.contextMenu(screen.getByRole('button', { name: 'Tidy' }))
    expect(contextMenuStore.get()?.entries[0]?.label).toBe('Show Details')
  })

  it('says in the tooltip why an extension stopped', () => {
    expect(rowTooltip({ ...tidy, state: 'failed', error: 'trap: unreachable' })).toContain(
      'Stopped · started in 42 ms\ntrap: unreachable',
    )
  })
})

describe('ExtensionPage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(ipc.getExtensionReadme).mockResolvedValue({
      nodes: [
        { parent: null, type: 'heading', level: 1 },
        { parent: 0, type: 'text', text: 'Tidy keeps whitespace tidy' },
      ],
      highlights: {},
      highlightsPending: false,
    })
  })

  it('shows the README, details, permissions, commands, and problems', async () => {
    extensionsStore.set([tidy])
    render(<ExtensionPage id="example.tidy" />)
    const page = screen.getByRole('article', { name: 'Tidy' })
    expect(within(page).getByRole('heading', { level: 1, name: /Tidy/ })).toHaveTextContent(
      'Tidy0.1.0development',
    )
    expect(page).toHaveTextContent('Ferrite · example.tidy')
    expect(page).toHaveTextContent('Active · started in 42 ms')
    const readme = within(page).getByRole('region', { name: 'README' })
    expect(await within(readme).findByText('Tidy keeps whitespace tidy')).toBeInTheDocument()
    expect(ipc.getExtensionReadme).toHaveBeenCalledWith('example.tidy')
    expect(page).toHaveTextContent('1 color theme')
    expect(page).toHaveTextContent('Ran 12 times for 3.4 ms in total')
    expect(page).toHaveTextContent('Compiled in 30 ms')
    expect(page).toHaveTextContent('/ext/tidy')
    expect(within(page).getByRole('button', { name: 'Trim' })).toBeEnabled()
    expect(page).toHaveTextContent('width must be at least 0')
  })

  it('changes permissions and turns the extension off', async () => {
    const user = userEvent.setup()
    extensionsStore.set([tidy])
    render(<ExtensionPage id="example.tidy" />)
    await user.click(screen.getByRole('checkbox', { name: 'Edit open documents' }))
    expect(ipc.setExtensionPermissions).toHaveBeenCalledWith('example.tidy', [
      'documents-read',
      'documents-write',
    ])
    await user.click(screen.getByRole('button', { name: 'Disable' }))
    expect(ipc.setExtensionEnabled).toHaveBeenCalledWith('example.tidy', false)
    await user.click(screen.getByRole('button', { name: 'Restart' }))
    expect(ipc.restartExtension).toHaveBeenCalledWith('example.tidy')
  })

  it("shows a broken extension's problem, and that it has no README", () => {
    extensionsStore.set([broken])
    render(<ExtensionPage id="/ext/broken" />)
    expect(screen.getByRole('alert')).toHaveTextContent('requires extension API version 2')
    expect(screen.getByText('This extension has no README.')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Disable' })).not.toBeInTheDocument()
    expect(ipc.getExtensionReadme).not.toHaveBeenCalled()
  })

  it('names the languages an extension adds', () => {
    const server = { command: 'server', args: ['--stdio'], languages: ['html'], rootMarkers: [] }
    extensionsStore.set([
      {
        ...tidy,
        state: 'ready',
        contributions: {
          ...tidy.contributions,
          themes: [],
          languages: [
            { id: 'html', name: 'HTML', fileExtensions: ['html'], fileNames: [] },
            { id: 'css', name: 'CSS', fileExtensions: ['css'], fileNames: [] },
          ],
          languageServers: [
            { ...server, name: 'html' },
            { ...server, name: 'css' },
          ],
        },
      },
    ])
    render(<ExtensionPage id="example.tidy" />)
    expect(screen.getByText('Languages: HTML, CSS')).toBeInTheDocument()
    expect(screen.getByText('2 language servers')).toBeInTheDocument()
  })

  it('says so when the extension is gone', () => {
    extensionsStore.set([])
    render(<ExtensionPage id="example.tidy" />)
    expect(screen.getByText('This extension is no longer loaded.')).toBeInTheDocument()
  })
})
