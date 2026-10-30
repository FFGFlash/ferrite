import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { DocumentSummary } from '../ipc/editor'

vi.mock('../ipc/workspace', () => ({ listDirectory: vi.fn() }))

const ipc = await import('../ipc/workspace')
const { DEFAULT_LAYOUT, layoutStore } = await import('../state/layout')
const { workspaceStore } = await import('../state/workspace')
const { explorerStore, EXPLORER_TREE_ID } = await import('../panels/explorerState')
const { Breadcrumbs, crumbsFor } = await import('./Breadcrumbs')
const { setViewHandler } = await import('../state/viewRequests')
const showView = vi.fn()

const document = (path: string | null, title = 'Palette.tsx'): DocumentSummary => ({
  id: 1,
  title,
  path,
  dirty: false,
  lineEnding: 'LF',
  encoding: 'UTF-8',
})

describe('crumbsFor', () => {
  it('splits a path inside the workspace into revealable parts', () => {
    expect(crumbsFor('/w/src/components/Palette.tsx', 'Palette.tsx', '/w')).toEqual([
      { label: 'src', path: '/w/src' },
      { label: 'components', path: '/w/src/components' },
      { label: 'Palette.tsx', path: '/w/src/components/Palette.tsx' },
    ])
    expect(crumbsFor('C:\\w\\src\\main.rs', 'main.rs', 'C:\\w')).toEqual([
      { label: 'src', path: 'C:\\w\\src' },
      { label: 'main.rs', path: 'C:\\w\\src\\main.rs' },
    ])
  })

  it('shows other files by their full path, and untitled ones by title', () => {
    expect(crumbsFor('C:\\notes\\a.txt', 'a.txt', 'C:\\w')).toEqual([
      { label: 'C:', path: null },
      { label: 'notes', path: null },
      { label: 'a.txt', path: null },
    ])
    expect(crumbsFor('/tmp/a.txt', 'a.txt', null).map(crumb => crumb.label)).toEqual([
      'tmp',
      'a.txt',
    ])
    expect(crumbsFor(null, 'Untitled-1', '/w')).toEqual([{ label: 'Untitled-1', path: null }])
  })
})

describe('Breadcrumbs', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(ipc.listDirectory).mockResolvedValue([])
    layoutStore.set(DEFAULT_LAYOUT)
    setViewHandler(showView)
    workspaceStore.set({ info: { root: '/w', name: 'w' }, listings: new Map(), indexing: false })
  })

  it('lists the path with the current file last', () => {
    render(<Breadcrumbs document={document('/w/src/components/Palette.tsx')} />)
    const nav = screen.getByRole('navigation', { name: /^Breadcrumbs, / })
    const parts = within(nav).getAllByRole('button')
    expect(parts.map(part => part.textContent)).toEqual(['src', 'components', 'Palette.tsx'])
    expect(parts[2]).toHaveAttribute('aria-current', 'location')
    expect(parts[2]?.querySelector('.breadcrumbs__icon')).not.toBeNull()
  })

  it('reveals a part in the explorer and moves focus there', async () => {
    const user = userEvent.setup()
    const tree = window.document.createElement('ul')
    tree.id = EXPLORER_TREE_ID
    tree.tabIndex = 0
    window.document.body.append(tree)
    try {
      render(<Breadcrumbs document={document('/w/src/components/Palette.tsx')} />)
      await user.click(screen.getByRole('button', { name: 'components' }))
      expect(showView).toHaveBeenCalledWith('explorer', false)
      await waitFor(() => expect(explorerStore.get().focused).toBe('/w/src/components'))
      expect(layoutStore.get().expanded).toEqual(['/w/src'])
      await waitFor(() => expect(tree).toHaveFocus())
    } finally {
      tree.remove()
    }
  })

  it("doesn't offer to reveal files outside the workspace", () => {
    render(<Breadcrumbs document={document('/elsewhere/notes.txt', 'notes.txt')} />)
    const nav = screen.getByRole('navigation', { name: /^Breadcrumbs, / })
    expect(within(nav).queryByRole('button')).not.toBeInTheDocument()
    expect(nav).toHaveTextContent('elsewherenotes.txt')
  })

  it('follows the workspace', () => {
    render(<Breadcrumbs document={document('/w/src/main.rs', 'main.rs')} />)
    expect(screen.getAllByRole('button')).toHaveLength(2)
    act(() => workspaceStore.set({ info: null, listings: new Map(), indexing: false }))
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })
})
