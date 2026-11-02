import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { DocumentSummary } from '../ipc/editor'

vi.mock('../ipc/terminal', () => ({
  createTerminal: vi.fn(),
  listTerminals: vi.fn(),
  attachTerminal: vi.fn(),
  writeTerminal: vi.fn(),
  resizeTerminal: vi.fn(),
  acknowledgeTerminal: vi.fn(),
  killTerminal: vi.fn(),
  terminalPlatform: vi.fn(async () => ({ windowsBuild: null })),
}))
vi.mock('../ipc/editor', () => ({
  closeDocument: vi.fn(async () => true),
  openDocument: vi.fn(),
}))
vi.mock('../ipc/extensions', () => ({
  listPreviews: vi.fn(async () => []),
  previewClosed: vi.fn(async () => {}),
}))

const terminalIpc = await import('../ipc/terminal')
const editorIpc = await import('../ipc/editor')
const extensionIpc = await import('../ipc/extensions')
const previews = await import('../state/previews')
const dock = await import('./dock')
const { findGroup, groupOf, groups, sidebarGroups } = await import('./model')
const { documentsStore } = await import('../state/documents')
const { DEFAULT_LAYOUT, layoutStore } = await import('../state/layout')
const terminals = await import('../terminal/terminals')
const { showPage } = await import('../state/viewRequests')

const doc = (id: number, path: string | null = `/w/file${id}.rs`): DocumentSummary => ({
  id,
  title: path?.split('/').pop() ?? `Untitled-${id}`,
  path,
  dirty: false,
  lineEnding: 'LF',
  encoding: 'UTF-8',
})

const preview = (id: string, document: number | null) => ({
  id,
  extension: 'Markdown',
  title: `Preview ${id}`,
  icon: 'book',
  document,
})

const layout = () => dock.dockStore.get()
const groupItems = (id: string) => findGroup(layout(), id)?.items
const groupHolding = (item: string) => groupOf(layout(), item)
let nextTerminal = 1

describe('dock', () => {
  beforeEach(() => {
    for (const tab of terminals.terminalsStore.get().tabs) {
      terminals.closeTerminal(tab.id)
    }
    documentsStore.set({ documents: [], activeId: null })
    previews.previewsStore.set([])
    vi.clearAllMocks()
    vi.mocked(terminalIpc.createTerminal).mockImplementation(async () => ({
      id: nextTerminal++,
      title: 'pwsh',
    }))
    vi.mocked(terminalIpc.killTerminal).mockResolvedValue(undefined)
    // Reopening a file gives it a new id, as the backend does.
    let nextDocument = 100
    vi.mocked(editorIpc.openDocument).mockImplementation(async path => doc(nextDocument++, path))
    vi.mocked(terminalIpc.listTerminals).mockResolvedValue([])
    layoutStore.set(DEFAULT_LAYOUT)
    dock.restoreDock()
  })

  it('starts with the views on the left and an empty main group', () => {
    expect(groups(layout()).map(group => [group.id, group.items])).toEqual([
      ['g1', ['view:explorer', 'view:search', 'view:extensions']],
      ['main', []],
    ])
  })

  it('follows open documents: new ones in the main group, closed ones removed', () => {
    documentsStore.set({ documents: [doc(1), doc(2)], activeId: 2 })
    expect(groupItems('main')).toEqual(['document:1', 'document:2'])
    expect(findGroup(layout(), 'main')!.active).toBe('document:2')

    documentsStore.set({ documents: [doc(2)], activeId: 2 })
    expect(groupItems('main')).toEqual(['document:2'])
  })

  it('opens pages where documents open, once, and saves them', () => {
    documentsStore.set({ documents: [doc(1), doc(2)], activeId: 2 })
    dock.moveItem('document:2', { kind: 'split', group: 'main', edge: 'right' })
    const side = groupHolding('document:2')!.id

    showPage('settings')
    showPage('settings')
    expect(groupItems(side)).toEqual(['document:2', 'page:settings'])
    expect(findGroup(layout(), side)!.active).toBe('page:settings')
    expect(JSON.stringify(dock.serializeDock())).toContain('page:settings')

    dock.closeItem('page:settings')
    expect(groupItems(side)).toEqual(['document:2'])
  })

  it('opens documents in the group last used for documents', () => {
    documentsStore.set({ documents: [doc(1), doc(2)], activeId: 2 })
    dock.moveItem('document:2', { kind: 'split', group: 'main', edge: 'right' })
    const side = groupHolding('document:2')!.id
    expect(side).not.toBe('main')
    expect(dock.currentGroup.get()).toBe(side)

    documentsStore.update(state => ({ documents: [...state.documents, doc(3)], activeId: 3 }))
    expect(groupHolding('document:3')!.id).toBe(side)
  })

  it('opens previews with their document, or beside it, and saves neither', async () => {
    documentsStore.set({ documents: [doc(1), doc(2)], activeId: 1 })
    dock.moveItem('document:2', { kind: 'split', group: 'main', edge: 'bottom' })
    const second = groupHolding('document:2')!.id

    vi.mocked(extensionIpc.listPreviews).mockResolvedValue([preview('p/2', 2)])
    await previews.showPreviewRequested('p/2', false)
    expect(groupItems(second)).toEqual(['document:2', 'preview:p/2'])
    expect(findGroup(layout(), second)!.active).toBe('preview:p/2')

    vi.mocked(extensionIpc.listPreviews).mockResolvedValue([preview('p/2', 2), preview('p/1', 1)])
    dock.activateTab('document:1')
    await previews.showPreviewRequested('p/1', true)
    const beside = groupHolding('preview:p/1')!
    expect(beside.id).not.toBe('main')
    expect(beside.items).toEqual(['preview:p/1'])
    expect(dock.currentGroup.get(), 'focus stays with the document').toBe('main')

    const saved = JSON.stringify(dock.serializeDock())
    expect(saved).not.toContain('preview:')
  })

  it('closes a preview tab, telling the backend, and follows previews it closes', async () => {
    documentsStore.set({ documents: [doc(1)], activeId: 1 })
    vi.mocked(extensionIpc.listPreviews).mockResolvedValue([preview('p/1', 1), preview('q', null)])
    await previews.refreshPreviews()
    expect(groupItems('main')).toEqual(['document:1', 'preview:p/1', 'preview:q'])

    dock.closeItem('preview:p/1')
    expect(extensionIpc.previewClosed).toHaveBeenCalledWith('p/1')
    expect(groupItems('main')).toEqual(['document:1', 'preview:q'])

    // The backend closed it (its document or extension did).
    vi.mocked(extensionIpc.listPreviews).mockResolvedValue([])
    await previews.refreshPreviews()
    expect(groupItems('main')).toEqual(['document:1'])
  })

  it('activating a document shows its tab and makes its group current', () => {
    documentsStore.set({ documents: [doc(1), doc(2)], activeId: 2 })
    dock.moveItem('document:1', { kind: 'split', group: 'main', edge: 'bottom' })
    dock.groupFocused('main')
    documentsStore.update(state => ({ ...state, activeId: 1 }))
    expect(dock.currentGroup.get()).toBe(groupHolding('document:1')!.id)
  })

  it('puts terminals together, docked along the bottom at first', async () => {
    await dock.openTerminal()
    await dock.openTerminal()
    const group = groupHolding('terminal:1')!
    expect(group.items).toHaveLength(2)
    const root = layout().root
    expect(root.kind === 'split' && root.direction).toBe('column')
    expect(root.kind === 'split' && root.children.at(-1)).toBe(group)
  })

  it('hides a shown view group from the activity bar, and shows it again', () => {
    dock.toggleItem('view:explorer')
    expect(findGroup(layout(), 'g1')!.hidden).toBe(true)
    dock.toggleItem('view:explorer')
    expect(findGroup(layout(), 'g1')!.hidden).toBe(false)

    // Another view in the group is switched to, not hidden.
    dock.toggleItem('view:search')
    expect(findGroup(layout(), 'g1')).toMatchObject({ hidden: false, active: 'view:search' })
  })

  it('toggles the side bar, and every terminal group', async () => {
    dock.toggleSidebar()
    expect(findGroup(layout(), 'g1')!.hidden).toBe(true)
    dock.toggleSidebar()
    expect(findGroup(layout(), 'g1')!.hidden).toBe(false)

    await dock.toggleTerminals()
    expect(terminalIpc.createTerminal).toHaveBeenCalledOnce()
    const terminalGroup = groupHolding(dock.terminalItem(terminals.terminalsStore.get().activeId!))!
    await dock.toggleTerminals()
    expect(findGroup(layout(), terminalGroup.id)!.hidden).toBe(true)
    await dock.toggleTerminals()
    expect(findGroup(layout(), terminalGroup.id)!.hidden).toBe(false)
    expect(terminalIpc.createTerminal).toHaveBeenCalledOnce()
  })

  it('shows what is in the side bar, and the views not docked, on the activity bar', async () => {
    const bar = () => dock.activityBarItems(layout(), ['explorer', 'search', 'extensions', 'todos'])
    expect(bar()).toEqual(['view:explorer', 'view:search', 'view:extensions', 'view:todos'])

    dock.moveItem('view:explorer', { kind: 'root', edge: 'right' })
    expect(bar()).toEqual(['view:search', 'view:extensions', 'view:todos'])

    // Tab order is button order; views not docked follow, in their usual order.
    dock.moveItem('view:extensions', { kind: 'tabs', group: 'g1', index: 0 })
    dock.closeItem('view:search')
    dock.moveItem('view:explorer', { kind: 'tabs', group: 'g1', index: 2 })
    dock.closeItem('view:explorer')
    expect(bar()).toEqual(['view:extensions', 'view:explorer', 'view:search', 'view:todos'])

    // Terminals and documents on the left are there too; along the bottom, they aren't.
    await dock.openTerminal()
    const terminal = dock.terminalItem(terminals.terminalsStore.get().activeId!)
    expect(bar()).not.toContain(terminal)
    dock.moveItem(terminal, { kind: 'split', group: 'g1', edge: 'bottom' })
    documentsStore.set({ documents: [doc(1)], activeId: 1 })
    dock.moveItem('document:1', { kind: 'tabs', group: 'g1', index: 1 })
    expect(bar()).toEqual([
      'view:extensions',
      'document:1',
      terminal,
      'view:explorer',
      'view:search',
      'view:todos',
    ])
  })

  it('opens views from the activity bar in the side bar', () => {
    dock.moveItem('view:explorer', { kind: 'root', edge: 'right' })
    const right = groupHolding('view:explorer')!.id
    dock.closeItem('view:explorer')

    // Not where it was closed: its button is in the side bar.
    dock.toggleItem('view:explorer')
    expect(groupHolding('view:explorer')!.id).toBe('g1')
    expect(right).not.toBe('g1')

    // With nothing on the left, a new side bar group.
    dock.moveItem('view:search', { kind: 'tabs', group: 'main', index: 0 })
    dock.moveItem('view:extensions', { kind: 'tabs', group: 'main', index: 0 })
    dock.moveItem('view:explorer', { kind: 'tabs', group: 'main', index: 0 })
    dock.toggleItem('view:todos')
    const root = layout().root
    expect(root.kind === 'split' && root.children[0]).toBe(groupHolding('view:todos'))
  })

  it('moves an item dropped on the activity bar into the side bar', async () => {
    dock.moveItem('view:search', { kind: 'root', edge: 'right' })
    dock.dockInSidebar('view:search')
    expect(groupItems('g1')).toEqual(['view:explorer', 'view:extensions', 'view:search'])
    expect(findGroup(layout(), 'g1')!.active).toBe('view:search')

    // Other kinds get a group of their own below, keeping the views beside the activity bar.
    const activeTerminal = () => dock.terminalItem(terminals.terminalsStore.get().activeId!)
    await dock.openTerminal()
    const first = activeTerminal()
    dock.dockInSidebar(first)
    const sidebar = sidebarGroups(layout())
    expect(sidebar.map(group => group.items)).toEqual([
      ['view:explorer', 'view:extensions', 'view:search'],
      [first],
    ])
    await dock.openTerminal()
    const second = activeTerminal()
    dock.dockInSidebar(second)
    expect(groupItems(sidebar[1]!.id)).toEqual([first, second])
  })

  it('toggles only the side bar, whatever is in it', async () => {
    dock.moveItem('view:search', { kind: 'root', edge: 'right' })
    const right = groupHolding('view:search')!.id
    await dock.openTerminal()
    const terminal = dock.terminalItem(terminals.terminalsStore.get().activeId!)
    dock.dockInSidebar(terminal)
    const terminalGroup = groupHolding(terminal)!.id

    dock.toggleSidebar()
    expect(findGroup(layout(), 'g1')!.hidden).toBe(true)
    expect(findGroup(layout(), terminalGroup)!.hidden).toBe(true)
    expect(findGroup(layout(), right)!.hidden).toBeFalsy()
    dock.toggleSidebar()
    expect(findGroup(layout(), 'g1')!.hidden).toBe(false)
    expect(findGroup(layout(), terminalGroup)!.hidden).toBe(false)
  })

  it('reopens a closed view where it was', () => {
    dock.closeItem('view:search')
    expect(groupItems('g1')).toEqual(['view:explorer', 'view:extensions'])
    dock.showView('search', false)
    expect(groupItems('g1')).toEqual(['view:explorer', 'view:search', 'view:extensions'])
  })

  it('cycles and closes tabs in the current group', () => {
    dock.groupFocused('g1')
    dock.cycleTab(1)
    expect(findGroup(layout(), 'g1')!.active).toBe('view:search')
    dock.cycleTab(-2)
    expect(findGroup(layout(), 'g1')!.active).toBe('view:extensions')
    dock.closeCurrentTab()
    expect(groupItems('g1')).toEqual(['view:explorer', 'view:search'])
  })

  it('splits the current tab into a new group', () => {
    dock.groupFocused('g1')
    dock.splitCurrentTab('bottom')
    const group = groupHolding('view:explorer')!
    expect(group.id).not.toBe('g1')
    expect(groupItems('g1')).toEqual(['view:search', 'view:extensions'])
    expect(dock.currentGroup.get()).toBe(group.id)
  })

  it('saves documents by path and puts them back in place', () => {
    documentsStore.set({ documents: [doc(1), doc(2, null)], activeId: 1 })
    dock.moveItem('document:1', { kind: 'tabs', group: 'g1', index: 0 })
    const saved = dock.serializeDock()
    // Untitled documents aren't saved.
    expect(findGroup(saved, 'g1')!.items[0]).toBe('file:/w/file1.rs')
    expect(findGroup(saved, 'main')!.items).toEqual([])

    // The next run: the document has another id.
    documentsStore.set({ documents: [], activeId: null })
    layoutStore.set({ ...DEFAULT_LAYOUT, dock: JSON.parse(JSON.stringify(saved)) })
    dock.restoreDock()
    documentsStore.set({ documents: [doc(7, '/w/file1.rs'), doc(8)], activeId: 7 })
    dock.documentsRestored()
    expect(groupItems('g1')![0]).toBe('document:7')
    expect(groupItems('main')).toEqual(['document:8'])
  })

  it('drops saved documents that do not open again', () => {
    layoutStore.set({
      ...DEFAULT_LAYOUT,
      dock: {
        main: 'main',
        root: { kind: 'group', id: 'main', items: ['file:/gone.rs'], active: 'file:/gone.rs' },
      },
    })
    dock.restoreDock()
    dock.documentsRestored()
    expect(groupItems('main')).toEqual([])
  })

  it('seeds the default from the side bar of layouts from before the dock', () => {
    layoutStore.set({ ...DEFAULT_LAYOUT, dock: { sidebarVisible: false, activeView: 'search' } })
    dock.restoreDock()
    expect(findGroup(layout(), 'g1')).toMatchObject({ hidden: true, active: 'view:search' })
  })

  it('starts a terminal where the saved layout showed terminals', async () => {
    layoutStore.set({
      ...DEFAULT_LAYOUT,
      dock: {
        main: 'main',
        root: {
          kind: 'split',
          id: 's1',
          direction: 'column',
          sizes: [0.7, 0.3],
          children: [
            { kind: 'group', id: 'main', items: [], active: null },
            { kind: 'group', id: 'g5', items: ['terminal:41'], active: 'terminal:41' },
          ],
        },
      },
    })
    dock.restoreDock()
    // Asked twice at once, as React's development mode does: one terminal.
    await Promise.all([dock.terminalsRestored(), dock.terminalsRestored()])
    expect(terminalIpc.createTerminal).toHaveBeenCalledOnce()
    const started = dock.terminalItem(terminals.terminalsStore.get().activeId!)
    expect(groupItems('g5')).toEqual([started])
  })

  it('resets to the starting layout, keeping what is open', async () => {
    documentsStore.set({ documents: [doc(1)], activeId: 1 })
    await dock.openTerminal()
    dock.moveItem('document:1', { kind: 'root', edge: 'top' })
    dock.closeItem('view:search')
    dock.resetLayout()
    expect(groupItems('g1')).toEqual(['view:explorer', 'view:search', 'view:extensions'])
    expect(groupItems('main')).toEqual(['document:1'])
    expect(groupHolding(dock.terminalItem(terminals.terminalsStore.get().activeId!))).toBeDefined()
  })

  it('opens Problems and Output along the bottom, with the terminals', async () => {
    dock.showView('problems')
    const bottom = groupHolding('view:problems')!.id
    expect(sidebarGroups(layout()).some(group => group.id === bottom)).toBe(false)
    expect(bottom).not.toBe('main')

    await dock.openTerminal()
    dock.showView('output')
    expect(groupItems(bottom)).toEqual([
      'view:problems',
      'terminal:' + (nextTerminal - 1),
      'view:output',
    ])
    // Only in the activity bar when docked in the side bar.
    expect(dock.activityBarItems(layout(), ['explorer', 'problems', 'output'])).not.toContain(
      'view:problems',
    )

    // They show and hide with the terminals.
    await dock.toggleTerminals()
    expect(findGroup(layout(), bottom)!.hidden).toBe(true)
    await dock.toggleTerminals()
    expect(findGroup(layout(), bottom)!.hidden).toBeFalsy()
  })

  it('reopens closed tabs, the latest first, where they were', async () => {
    documentsStore.set({ documents: [doc(1), doc(2)], activeId: 1 })
    showPage('settings')
    expect(groupItems('main')).toEqual(['document:1', 'page:settings', 'document:2'])
    dock.closeItem('view:search')
    dock.closeItem('page:settings')
    dock.closeItem('document:1')
    await vi.waitFor(() => expect(groupItems('main')).toEqual(['document:2']))

    await dock.reopenClosedTab()
    expect(editorIpc.openDocument).toHaveBeenCalledWith('/w/file1.rs')
    expect(groupItems('main')).toEqual(['document:100', 'document:2'])
    expect(findGroup(layout(), 'main')!.active).toBe('document:100')
    await dock.reopenClosedTab()
    expect(groupItems('main')).toEqual(['document:100', 'page:settings', 'document:2'])
    await dock.reopenClosedTab()
    expect(groupItems('g1')).toEqual(['view:explorer', 'view:search', 'view:extensions'])
    expect(dock.hasClosedTab()).toBe(false)
  })

  it('skips tabs open again, and keeps no untitled documents or terminals', async () => {
    documentsStore.set({ documents: [doc(1, null)], activeId: 1 })
    await dock.openTerminal()
    const terminal = groupHolding('terminal:' + terminals.terminalsStore.get().activeId)!
    dock.closeItem('view:search')
    dock.groupFocused(terminal.id)
    await dock.closeGroupTabs()
    dock.groupFocused('main')
    await dock.closeGroupTabs()
    expect(dock.hasClosedTab()).toBe(true)

    dock.showView('search', false)
    await dock.reopenClosedTab()
    expect(dock.hasClosedTab()).toBe(false)
    expect(editorIpc.openDocument).not.toHaveBeenCalled()
    expect(terminals.terminalsStore.get().tabs).toEqual([])
  })

  it('removes the place kept for a file that fails to reopen', async () => {
    documentsStore.set({ documents: [doc(1), doc(2)], activeId: 1 })
    dock.closeItem('document:1')
    await vi.waitFor(() => expect(groupItems('main')).toEqual(['document:2']))
    vi.mocked(editorIpc.openDocument).mockRejectedValueOnce(new Error('gone'))
    await dock.reopenClosedTab()
    expect(groupItems('main')).toEqual(['document:2'])
  })

  it('closes the saved, other, or all tabs of the group, and every editor', async () => {
    documentsStore.set({ documents: [doc(1), { ...doc(2), dirty: true }, doc(3)], activeId: 3 })
    showPage('settings')
    dock.groupFocused('main')
    await dock.closeSavedTabs()
    expect(groupItems('main')).toEqual(['document:2'])

    dock.groupFocused('g1')
    dock.activateTabNumber(3)
    await dock.closeOtherTabs()
    expect(groupItems('g1')).toEqual(['view:extensions'])

    await dock.closeAllEditors()
    expect(groupItems('main')).toEqual([])
    expect(groupItems('g1')).toEqual(['view:extensions'])

    // Closed from the last, so they reopen in order.
    documentsStore.set({ documents: [doc(4), doc(5), doc(6)], activeId: 6 })
    dock.groupFocused('main')
    await dock.closeGroupTabs()
    for (let i = 0; i < 3; i++) {
      await dock.reopenClosedTab()
    }
    expect(
      groupItems('main')!.map(
        item => documentsStore.get().documents.find(d => `document:${d.id}` === item)?.path,
      ),
    ).toEqual(['/w/file4.rs', '/w/file5.rs', '/w/file6.rs'])
  })

  it('stops closing tabs when closing one is cancelled', async () => {
    documentsStore.set({ documents: [doc(1), doc(2), doc(3)], activeId: 1 })
    vi.mocked(editorIpc.closeDocument).mockImplementation(async id => id !== 2)
    dock.groupFocused('main')
    await dock.closeGroupTabs()
    expect(groupItems('main')).toEqual(['document:1', 'document:2'])
    vi.mocked(editorIpc.closeDocument).mockImplementation(async () => true)
  })

  it('activates a tab of the current group by number, 0 for the last', () => {
    dock.groupFocused('g1')
    dock.activateTabNumber(2)
    expect(findGroup(layout(), 'g1')!.active).toBe('view:search')
    dock.activateTabNumber(0)
    expect(findGroup(layout(), 'g1')!.active).toBe('view:extensions')
    dock.activateTabNumber(9)
    expect(findGroup(layout(), 'g1')!.active).toBe('view:extensions')
  })
})
