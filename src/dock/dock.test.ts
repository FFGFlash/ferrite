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
vi.mock('../ipc/editor', () => ({ closeDocument: vi.fn(async () => true) }))

const terminalIpc = await import('../ipc/terminal')
const dock = await import('./dock')
const { findGroup, groupOf, groups } = await import('./model')
const { documentsStore } = await import('../state/documents')
const { DEFAULT_LAYOUT, layoutStore } = await import('../state/layout')
const terminals = await import('../terminal/terminals')

const doc = (id: number, path: string | null = `/w/file${id}.rs`): DocumentSummary => ({
  id,
  title: path?.split('/').pop() ?? `Untitled-${id}`,
  path,
  dirty: false,
  lineEnding: 'LF',
  encoding: 'UTF-8',
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
    vi.clearAllMocks()
    vi.mocked(terminalIpc.createTerminal).mockImplementation(async () => ({
      id: nextTerminal++,
      title: 'pwsh',
    }))
    vi.mocked(terminalIpc.killTerminal).mockResolvedValue(undefined)
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

  it('opens documents in the group last used for documents', () => {
    documentsStore.set({ documents: [doc(1), doc(2)], activeId: 2 })
    dock.moveItem('document:2', { kind: 'split', group: 'main', edge: 'right' })
    const side = groupHolding('document:2')!.id
    expect(side).not.toBe('main')
    expect(dock.currentGroup.get()).toBe(side)

    documentsStore.update(state => ({ documents: [...state.documents, doc(3)], activeId: 3 }))
    expect(groupHolding('document:3')!.id).toBe(side)
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
    dock.toggleView('explorer')
    expect(findGroup(layout(), 'g1')!.hidden).toBe(true)
    dock.toggleView('explorer')
    expect(findGroup(layout(), 'g1')!.hidden).toBe(false)

    // Another view in the group is switched to, not hidden.
    dock.toggleView('search')
    expect(findGroup(layout(), 'g1')).toMatchObject({ hidden: false, active: 'view:search' })
  })

  it('toggles every view group, and every terminal group', async () => {
    dock.toggleViews()
    expect(findGroup(layout(), 'g1')!.hidden).toBe(true)
    dock.toggleViews()
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
})
