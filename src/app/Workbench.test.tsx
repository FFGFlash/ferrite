import { act, fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { contextMenuStore } from '../components/contextMenuState'
import { dockStore, moveItem, restoreDock } from '../dock/dock'
import { findGroup, groupOf } from '../dock/model'
import type { AppInfo } from '../ipc'
import { documentsStore } from '../state/documents'
import { DEFAULT_LAYOUT, layoutStore } from '../state/layout'
import { Workbench } from './Workbench'

const appInfo: AppInfo = {
  name: 'Ferrite',
  version: '0.1.0',
  os: 'windows',
  arch: 'x86_64',
  tauriVersion: '2.12.5',
}

/** jsdom has no layout: gives every element a 1000×1000 box. */
function stubLayout() {
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue(
    DOMRect.fromRect({ x: 0, y: 0, width: 1000, height: 1000 }),
  )
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(1000)
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(1000)
}

/** A `dataTransfer` that carries data from dragstart to drop, as browsers do. */
function dataTransfer() {
  const data = new Map<string, string>()
  return {
    get types() {
      return [...data.keys()]
    },
    setData: (type: string, value: string) => data.set(type, value),
    getData: (type: string) => data.get(type) ?? '',
    effectAllowed: 'all',
    dropEffect: 'none',
  }
}

const groupHolding = (item: string) => groupOf(dockStore.get(), item)

/**
 * Fires a drag event at a point. jsdom has no `DragEvent`, so a mouse event
 * carries the coordinates and the `dataTransfer`.
 */
function drag(
  type: 'dragOver' | 'drop',
  element: Element,
  transfer: ReturnType<typeof dataTransfer>,
  x: number,
  y: number,
) {
  const event = new MouseEvent(type.toLowerCase(), {
    bubbles: true,
    cancelable: true,
    clientX: x,
    clientY: y,
  })
  Object.defineProperty(event, 'dataTransfer', { value: transfer })
  fireEvent(element, event)
}

describe('Workbench', () => {
  beforeEach(() => {
    // Documents first: closing them updates the saved dock, which the reset replaces.
    documentsStore.set({ documents: [], activeId: null })
    layoutStore.set(DEFAULT_LAYOUT)
    restoreDock()
  })

  afterEach(async () => {
    // Focus moves on the next frame; don't let it land in the next test.
    await act(() => new Promise(resolve => requestAnimationFrame(resolve)))
    vi.restoreAllMocks()
  })

  it('opens with the views on the left and the welcome page', () => {
    render(<Workbench appInfo={null} />)

    const views = screen.getByRole('region', { name: 'Explorer' })
    expect(
      within(views)
        .getAllByRole('tab')
        .map(tab => tab.getAttribute('aria-label')),
    ).toEqual(['Explorer', 'Search', 'Extensions'])
    expect(screen.getByRole('button', { name: 'Explorer' })).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByRole('heading', { name: 'Ferrite' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /New File/ })).toBeInTheDocument()
  })

  it('switches views from the activity bar, and hides their group when selected again', async () => {
    const user = userEvent.setup()
    render(<Workbench appInfo={null} />)
    const search = screen.getByRole('button', { name: 'Search' })

    await user.click(search)
    expect(screen.getByRole('tab', { name: 'Search' })).toHaveAttribute('aria-selected', 'true')
    expect(search).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByRole('button', { name: 'Explorer' })).toHaveAttribute(
      'aria-pressed',
      'false',
    )

    await user.click(search)
    expect(screen.queryByRole('tab', { name: 'Search' })).not.toBeInTheDocument()
    expect(search).toHaveAttribute('aria-pressed', 'false')

    await user.click(screen.getByRole('button', { name: 'Explorer' }))
    expect(screen.getByRole('tab', { name: 'Explorer' })).toHaveAttribute('aria-selected', 'true')
  })

  it('shows the side bar views on the activity bar, and takes views dropped on it', () => {
    stubLayout()
    render(<Workbench appInfo={null} />)
    const activityBar = screen.getByRole('navigation', { name: 'Views' })
    const buttons = () =>
      within(activityBar)
        .getAllByRole('button')
        .map(button => button.getAttribute('aria-label'))
    expect(buttons()).toEqual(['Explorer', 'Search', 'Extensions'])

    // Docked on the right, the explorer leaves the activity bar.
    act(() => moveItem('view:explorer', { kind: 'root', edge: 'right' }))
    expect(buttons()).toEqual(['Search', 'Extensions'])

    // Its tab dropped on the activity bar brings it back into the side bar.
    const transfer = dataTransfer()
    fireEvent.dragStart(screen.getByRole('tab', { name: 'Explorer' }), { dataTransfer: transfer })
    drag('dragOver', activityBar, transfer, 10, 500)
    expect(activityBar).toHaveAttribute('data-drop', 'true')
    drag('drop', activityBar, transfer, 10, 500)
    expect(activityBar).not.toHaveAttribute('data-drop')
    expect(groupHolding('view:explorer')!.id).toBe('g1')
    expect(buttons()).toEqual(['Search', 'Extensions', 'Explorer'])

    // Anything docked there gets a button, such as a document.
    act(() =>
      documentsStore.set({
        documents: [
          {
            id: 1,
            title: 'main.rs',
            path: '/w/main.rs',
            dirty: false,
            lineEnding: 'LF',
            encoding: 'UTF-8',
          },
        ],
        activeId: 1,
      }),
    )
    expect(buttons()).toEqual(['Search', 'Extensions', 'Explorer'])
    fireEvent.dragStart(screen.getByRole('tab', { name: 'main.rs' }), { dataTransfer: transfer })
    drag('dragOver', activityBar, transfer, 10, 500)
    drag('drop', activityBar, transfer, 10, 500)
    expect(buttons()).toEqual(['Search', 'Extensions', 'Explorer', 'main.rs'])
    expect(within(activityBar).getByRole('button', { name: 'main.rs' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
  })

  it('resizes groups from the keyboard within their limits', async () => {
    stubLayout()
    const user = userEvent.setup()
    render(<Workbench appInfo={null} />)
    const sash = screen.getByRole('separator', { name: 'Resize group' })
    const sizes = () => {
      const root = dockStore.get().root
      return root.kind === 'split' ? root.sizes : []
    }

    sash.focus()
    await user.keyboard('{ArrowRight}')
    expect(sizes()[0]).toBeCloseTo(0.216)
    await user.keyboard('{End}')
    expect(sizes()[0]).toBeCloseTo(0.9)
    await user.keyboard('{Home}')
    expect(sizes()[0]).toBeCloseTo(0.1)
  })

  it('docks a dragged tab into another group, or beside it', () => {
    stubLayout()
    act(() =>
      documentsStore.set({
        documents: [
          {
            id: 1,
            title: 'main.rs',
            path: '/w/main.rs',
            dirty: false,
            lineEnding: 'LF',
            encoding: 'UTF-8',
          },
        ],
        activeId: 1,
      }),
    )
    render(<Workbench appInfo={null} />)
    const transfer = dataTransfer()
    const editor = screen.getByRole('region', { name: 'main.rs' })

    // Onto the editor's tabs: a tab of that group, even at the top edge of the window.
    fireEvent.dragStart(screen.getByRole('tab', { name: 'Search' }), { dataTransfer: transfer })
    const bar = within(editor).getByRole('tablist').parentElement!
    drag('dragOver', bar, transfer, 500, 5)
    drag('drop', bar, transfer, 500, 5)
    expect(groupHolding('view:search')!.id).toBe('main')

    // Onto the right side of the content: a new group there.
    fireEvent.dragStart(screen.getByRole('tab', { name: 'Explorer' }), { dataTransfer: transfer })
    const content = within(screen.getByRole('region', { name: 'Search' })).getByRole('tabpanel')
    drag('dragOver', content, transfer, 900, 500)
    expect(content.querySelector('.dock-drop')).toHaveAttribute('data-zone', 'right')
    drag('drop', content, transfer, 900, 500)
    const group = groupHolding('view:explorer')!
    expect(group.items).toEqual(['view:explorer'])
    expect(group.id).not.toBe('g1')

    // Near the edge of the whole dock, over a group along it: a group along that edge.
    fireEvent.dragStart(screen.getByRole('tab', { name: 'Extensions' }), {
      dataTransfer: transfer,
    })
    const outer = within(screen.getByRole('region', { name: 'Search' })).getByRole('tabpanel')
    drag('dragOver', outer, transfer, 500, 990)
    expect(document.querySelector('.dock-drop--root')).toHaveAttribute('data-zone', 'bottom')
    drag('drop', outer, transfer, 500, 990)
    const root = dockStore.get().root
    expect(root.kind === 'split' && root.direction).toBe('column')
    expect(root.kind === 'split' && root.children.at(-1)).toBe(groupHolding('view:extensions'))
  })

  it('moves between tabs with the arrow keys and closes them with Delete', async () => {
    const user = userEvent.setup()
    render(<Workbench appInfo={null} />)
    const explorer = screen.getByRole('tab', { name: 'Explorer' })

    explorer.focus()
    await user.keyboard('{ArrowRight}')
    expect(screen.getByRole('tab', { name: 'Search' })).toHaveAttribute('aria-selected', 'true')
    await act(() => new Promise(resolve => requestAnimationFrame(resolve)))
    expect(screen.getByRole('tab', { name: 'Search' })).toHaveFocus()

    await user.keyboard('{Delete}')
    expect(screen.queryByRole('tab', { name: 'Search' })).not.toBeInTheDocument()
    expect(findGroup(dockStore.get(), 'g1')!.items).toEqual(['view:explorer', 'view:extensions'])
  })

  it('offers splitting and moving in a tab menu, from the keyboard too', async () => {
    const user = userEvent.setup()
    render(<Workbench appInfo={null} />)

    screen.getByRole('tab', { name: 'Explorer' }).focus()
    await user.keyboard('{Shift>}{F10}{/Shift}')
    const labels = contextMenuStore.get()?.entries.map(entry => entry?.label ?? '—')
    expect(labels).toEqual([
      'Close Explorer',
      '—',
      'Split Right',
      'Split Down',
      '—',
      'Move Left',
      'Move Right',
      'Move Up',
      'Move Down',
    ])
    act(() => contextMenuStore.set(null))
  })

  it('shows backend application details once available', () => {
    const { rerender } = render(<Workbench appInfo={null} />)
    const statusBar = screen.getByRole('contentinfo')
    expect(within(statusBar).queryByText(/Ferrite/)).not.toBeInTheDocument()

    rerender(<Workbench appInfo={appInfo} />)
    expect(within(statusBar).getByText('Ferrite 0.1.0')).toBeInTheDocument()
    expect(screen.getByText('Windows x86_64')).toBeInTheDocument()
  })
})
