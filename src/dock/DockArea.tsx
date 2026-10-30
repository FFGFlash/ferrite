import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type DragEvent,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from 'react'
import { executeCommand } from '../commands/registry'
import { openContextMenu, type MenuEntry } from '../components/contextMenuState'
import { Sash } from '../components/Sash'
import { WelcomePage } from '../editor/WelcomePage'
import type { AppInfo } from '../ipc'
import { createStore, useStore } from '../state/store'
import {
  activateTab,
  closeItem,
  currentGroup,
  dockStore,
  focusItem,
  groupFocused,
  moveItem,
  resize,
} from './dock'
import {
  ItemActions,
  ItemContent,
  ItemIcon,
  itemLabel,
  itemTitle,
  itemTooltip,
  useResolver,
  type ResolvedItem,
} from './items'
import type { DockGroup, DockNode, DockSplit, DropTarget, Edge, ItemId } from './model'
import './Dock.css'

/** The data type of a dragged tab (or activity bar button). */
export const DOCK_ITEM_TYPE = 'application/x-ferrite-dock-item'

/** Groups are never squeezed smaller than this, in pixels. */
const MIN_GROUP_SIZE = 100
/** Dropping this close to the dock's edge docks along the whole edge. */
const ROOT_EDGE_DISTANCE = 24
/** Dropping within this share of a group's edge splits it. */
const SPLIT_ZONE = 0.25

/** What a drop here would do, for the preview. */
type DropHint =
  | { kind: 'split'; group: string; zone: Edge | 'center' }
  | { kind: 'root'; edge: Edge }
  | { kind: 'tabs'; group: string; index: number }

const dropHint = createStore<DropHint | null>(null)
/** The item being dragged, which `dataTransfer` doesn't reveal until the drop. */
const dragging = createStore<ItemId | null>(null)

/** Starts dragging an item: a tab, or a view from the activity bar. */
export function startDrag(event: DragEvent, item: ItemId): void {
  event.dataTransfer.setData(DOCK_ITEM_TYPE, item)
  event.dataTransfer.effectAllowed = 'move'
  dragging.set(item)
}

/**
 * Refuses drops of anything but dock items, such as files from another
 * program. HTML5 drag and drop needs Tauri's own drop handling turned off
 * (`dragDropEnabled`), and without it the webview would open a dropped file
 * in place of the workbench.
 */
export function installDropGuard(): void {
  for (const type of ['dragover', 'drop'] as const) {
    window.addEventListener(type, event => {
      if (!event.dataTransfer?.types.includes(DOCK_ITEM_TYPE)) {
        event.preventDefault()
        if (event.dataTransfer) {
          event.dataTransfer.dropEffect = 'none'
        }
      }
    })
  }
}

export function endDrag(): void {
  dragging.set(null)
  dropHint.set(null)
}

const isDockDrag = (event: DragEvent) => event.dataTransfer.types.includes(DOCK_ITEM_TYPE)

function sameHint(a: DropHint | null, b: DropHint | null): boolean {
  return JSON.stringify(a) === JSON.stringify(b)
}

function hint(next: DropHint | null): void {
  if (!sameHint(dropHint.get(), next)) {
    dropHint.set(next)
  }
}

/**
 * The edge of the whole dock the pointer is near, over a group's content: the
 * outer strip of a group that lies along that edge. Tab strips are always for
 * joining tabs, so the top edge is reached below them.
 */
function outerEdgeAt(event: DragEvent<HTMLElement>): Edge | null {
  const content = event.currentTarget
  const dock = content.closest('[data-dock]')?.getBoundingClientRect()
  const group = content.closest('[data-group]')?.getBoundingClientRect()
  if (!dock || !group) {
    return null
  }
  const r = content.getBoundingClientRect()
  const candidates: [Edge, boolean, number][] = [
    ['left', Math.abs(group.left - dock.left) < 2, event.clientX - r.left],
    ['right', Math.abs(dock.right - group.right) < 2, r.right - event.clientX],
    ['top', Math.abs(group.top - dock.top) < 2, event.clientY - r.top],
    ['bottom', Math.abs(dock.bottom - group.bottom) < 2, r.bottom - event.clientY],
  ]
  let best: Edge | null = null
  let bestDistance = ROOT_EDGE_DISTANCE
  for (const [edge, outer, distance] of candidates) {
    if (outer && distance <= bestDistance) {
      best = edge
      bestDistance = distance
    }
  }
  return best
}

/** The edge of the whole dock the pointer is near, for drags over its gaps (splitters). */
function rootEdgeAt(event: DragEvent): Edge | null {
  const dock = (event.currentTarget as HTMLElement).closest('[data-dock]')
  if (!dock) {
    return null
  }
  const r = dock.getBoundingClientRect()
  const distances: [Edge, number][] = [
    ['left', event.clientX - r.left],
    ['right', r.right - event.clientX],
    ['top', event.clientY - r.top],
    ['bottom', r.bottom - event.clientY],
  ]
  const [edge, distance] = distances.reduce((a, b) => (b[1] < a[1] ? b : a))
  return distance <= ROOT_EDGE_DISTANCE ? edge : null
}

function drop(event: DragEvent, target: DropTarget): void {
  event.preventDefault()
  const item = event.dataTransfer.getData(DOCK_ITEM_TYPE) || dragging.get()
  endDrag()
  if (item) {
    moveItem(item, target)
    focusItem(item)
  }
}

interface DockAreaProps {
  appInfo: AppInfo | null
}

/** Every group and split, filling the space beside the activity bar. */
export function DockArea({ appInfo }: DockAreaProps) {
  const layout = useStore(dockStore)
  const current = useStore(currentGroup)
  const resolve = useResolver()

  const shown = (node: DockNode): boolean =>
    node.kind === 'group'
      ? !node.hidden && (node.id === layout.main || node.items.some(item => resolve(item)))
      : node.children.some(shown)

  const render = (node: DockNode): ReactNode =>
    node.kind === 'group' ? (
      <GroupView
        key={node.id}
        group={node}
        main={node.id === layout.main}
        current={node.id === current}
        resolve={resolve}
        appInfo={appInfo}
      />
    ) : (
      <SplitView key={node.id} split={node} shown={shown} render={render} />
    )

  const onDragOver = (event: DragEvent<HTMLElement>) => {
    const edge = isDockDrag(event) ? rootEdgeAt(event) : null
    if (edge) {
      event.preventDefault()
      event.dataTransfer.dropEffect = 'move'
      hint({ kind: 'root', edge })
    }
  }

  return (
    <main
      className="dock"
      aria-label="Workbench"
      data-dock
      onDragOver={onDragOver}
      onDrop={event => {
        const edge = rootEdgeAt(event)
        if (edge && isDockDrag(event)) {
          drop(event, { kind: 'root', edge })
        }
      }}
      onDragLeave={event => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          hint(null)
        }
      }}
    >
      {shown(layout.root) ? render(layout.root) : render(groupNode(layout.main))}
      <RootDropPreview />
    </main>
  )
}

const groupNode = (id: string): DockGroup => ({ kind: 'group', id, items: [], active: null })

function RootDropPreview() {
  const current = useStore(dropHint)
  return current?.kind === 'root' ? (
    <div className="dock-drop dock-drop--root" data-zone={current.edge} />
  ) : null
}

interface SplitViewProps {
  split: DockSplit
  shown: (node: DockNode) => boolean
  render: (node: DockNode) => ReactNode
}

/** Children side by side (a row) or stacked (a column), with splitters between them. */
function SplitView({ split, shown, render }: SplitViewProps) {
  const ref = useRef<HTMLDivElement>(null)
  const [extent, setExtent] = useState(0)
  const row = split.direction === 'row'

  useLayoutEffect(() => {
    const element = ref.current
    if (!element) {
      return
    }
    const measure = () => setExtent(row ? element.clientWidth : element.clientHeight)
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  }, [row])

  const visible = split.children
    .map((node, index) => ({ node, index, size: split.sizes[index] ?? 0 }))
    .filter(entry => shown(entry.node))
  if (visible.length === 0) {
    return null
  }
  if (visible.length === 1) {
    return <>{render(visible[0]!.node)}</>
  }
  const total = visible.reduce((sum, entry) => sum + entry.size, 0) || 1
  const pixels = (entry: { size: number }) => Math.round((entry.size / total) * extent)

  /** Moves the boundary between visible children `k` and `k + 1`. */
  const resizePair = (k: number, firstPixels: number) => {
    const a = visible[k]!
    const b = visible[k + 1]!
    const pair = pixels(a) + pixels(b)
    const first = Math.min(pair - MIN_GROUP_SIZE, Math.max(MIN_GROUP_SIZE, firstPixels))
    const sizes = [...split.sizes]
    const share = (a.size + b.size) / pair
    sizes[a.index] = first * share
    sizes[b.index] = (pair - first) * share
    resize(split.id, sizes)
  }

  return (
    <div ref={ref} className={`dock-split dock-split--${split.direction}`}>
      {visible.map((entry, k) => {
        const next = visible[k + 1]
        const previous = visible[k - 1]
        return (
          <div
            key={entry.node.id}
            className="dock-split__cell"
            style={{ flex: `${entry.size / total} 1 0px` }}
          >
            {render(entry.node)}
            {/* A row's splitter sits on a cell's right edge; a column's on the top edge of the next cell. */}
            {row && next && extent > 0 && (
              <Sash
                label="Resize group"
                value={pixels(entry)}
                min={MIN_GROUP_SIZE}
                max={pixels(entry) + pixels(next) - MIN_GROUP_SIZE}
                onChange={value => resizePair(k, value)}
              />
            )}
            {!row && previous && extent > 0 && (
              <Sash
                label="Resize group"
                orientation="horizontal"
                value={pixels(entry)}
                min={MIN_GROUP_SIZE}
                max={pixels(previous) + pixels(entry) - MIN_GROUP_SIZE}
                onChange={value => resizePair(k - 1, pixels(previous) + pixels(entry) - value)}
              />
            )}
          </div>
        )
      })}
    </div>
  )
}

interface GroupViewProps {
  group: DockGroup
  main: boolean
  current: boolean
  resolve: (item: ItemId) => ResolvedItem | null
  appInfo: AppInfo | null
}

/** One group: its tabs, the active item's buttons, and the active item. */
function GroupView({ group, main, current, resolve, appInfo }: GroupViewProps) {
  const items = group.items
    .map(item => resolve(item))
    .filter((resolved): resolved is ResolvedItem => resolved !== null)
  const active = items.find(resolved => resolved.item === group.active) ?? items[0] ?? null
  const hintHere = useStore(dropHint)
  const zone = hintHere?.kind === 'split' && hintHere.group === group.id ? hintHere.zone : null
  const label = active ? itemTitle(active) : main ? 'Welcome' : 'Empty group'
  const panelId = `dock-panel-${group.id}`

  const onContentDragOver = (event: DragEvent<HTMLDivElement>) => {
    if (!isDockDrag(event)) {
      return
    }
    event.preventDefault()
    event.stopPropagation()
    event.dataTransfer.dropEffect = 'move'
    const outer = outerEdgeAt(event)
    if (outer) {
      hint({ kind: 'root', edge: outer })
      return
    }
    const r = event.currentTarget.getBoundingClientRect()
    const x = (event.clientX - r.left) / r.width
    const y = (event.clientY - r.top) / r.height
    const edges: [Edge, number][] = [
      ['left', x],
      ['right', 1 - x],
      ['top', y],
      ['bottom', 1 - y],
    ]
    const [edge, distance] = edges.reduce((a, b) => (b[1] < a[1] ? b : a))
    hint({ kind: 'split', group: group.id, zone: distance < SPLIT_ZONE ? edge : 'center' })
  }

  const onContentDrop = (event: DragEvent<HTMLDivElement>) => {
    if (!isDockDrag(event)) {
      return
    }
    event.stopPropagation()
    const outer = outerEdgeAt(event)
    const current = dropHint.get()
    const target: DropTarget = outer
      ? { kind: 'root', edge: outer }
      : current?.kind === 'split' && current.group === group.id && current.zone !== 'center'
        ? { kind: 'split', group: group.id, edge: current.zone }
        : { kind: 'tabs', group: group.id, index: group.items.length }
    drop(event, target)
  }

  return (
    <section
      className="dock-group"
      data-group={group.id}
      data-current={current || undefined}
      data-kind={active?.kind}
      aria-label={label}
      onFocus={() => groupFocused(group.id)}
    >
      {items.length > 0 && (
        <div
          className="dock-group__bar"
          onDragOver={event => {
            if (isDockDrag(event)) {
              event.preventDefault()
              event.stopPropagation()
              hint({ kind: 'tabs', group: group.id, index: group.items.length })
            }
          }}
          onDrop={event => {
            if (isDockDrag(event)) {
              event.stopPropagation()
              const current = dropHint.get()
              const index =
                current?.kind === 'tabs' && current.group === group.id
                  ? current.index
                  : group.items.length
              drop(event, { kind: 'tabs', group: group.id, index })
            }
          }}
        >
          <div className="dock-tabs" role="tablist" aria-label={`${label} tabs`}>
            {items.map(resolved => (
              <Tab
                key={resolved.item}
                resolved={resolved}
                group={group}
                active={resolved === active}
                panelId={panelId}
              />
            ))}
          </div>
          {active && (
            <div className="dock-group__actions">
              <ItemActions resolved={active} />
            </div>
          )}
        </div>
      )}
      <div
        className="dock-group__content"
        data-group-content={group.id}
        id={active ? panelId : undefined}
        role={active ? 'tabpanel' : undefined}
        aria-labelledby={active ? tabId(group.id, active.item) : undefined}
        tabIndex={-1}
        onDragOver={onContentDragOver}
        onDrop={onContentDrop}
      >
        {active ? (
          <ItemContent key={active.item} resolved={active} />
        ) : (
          <div className="dock-group__welcome">
            <WelcomePage appInfo={appInfo} />
          </div>
        )}
        {zone && <div className="dock-drop" data-zone={zone} />}
      </div>
    </section>
  )
}

const tabId = (group: string, item: ItemId) => `dock-tab-${group}-${item.replace(/[^\w-]/g, '_')}`

interface TabProps {
  resolved: ResolvedItem
  group: DockGroup
  active: boolean
  panelId: string
}

function Tab({ resolved, group, active, panelId }: TabProps) {
  const ref = useRef<HTMLDivElement>(null)
  const { item } = resolved
  const index = group.items.indexOf(item)
  const hintHere = useStore(dropHint)
  const dropSide =
    hintHere?.kind === 'tabs' && hintHere.group === group.id
      ? hintHere.index === index
        ? 'before'
        : hintHere.index === index + 1 && index === group.items.length - 1
          ? 'after'
          : null
      : null
  const title = itemTitle(resolved)

  useEffect(() => {
    if (active) {
      ref.current?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
    }
  }, [active])

  const menu = (x: number, y: number) => openContextMenu(x, y, tabMenu(item, title))

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = { ArrowLeft: -1, ArrowRight: 1 }[event.key]
    if (step !== undefined) {
      // Arrow keys move between tabs, keeping focus on the tabs.
      event.preventDefault()
      const tabs = [...(event.currentTarget.parentElement?.children ?? [])] as HTMLElement[]
      const next = tabs[(tabs.indexOf(event.currentTarget) + step + tabs.length) % tabs.length]
      const nextItem = next?.dataset.item
      if (nextItem) {
        activateTab(nextItem, false)
        requestAnimationFrame(() => document.getElementById(tabId(group.id, nextItem))?.focus())
      }
    } else if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      activateTab(item)
    } else if (event.key === 'Delete') {
      event.preventDefault()
      closeItem(item)
    } else if (event.key === 'ContextMenu' || (event.key === 'F10' && event.shiftKey)) {
      event.preventDefault()
      const r = event.currentTarget.getBoundingClientRect()
      menu(r.left, r.bottom)
    }
  }

  return (
    <div
      ref={ref}
      className="dock-tab"
      role="tab"
      id={tabId(group.id, item)}
      aria-selected={active}
      aria-controls={active ? panelId : undefined}
      aria-label={itemLabel(resolved)}
      tabIndex={active ? 0 : -1}
      title={itemTooltip(resolved)}
      data-kind={resolved.kind}
      data-item={item}
      data-dirty={(resolved.kind === 'document' && resolved.document.dirty) || undefined}
      data-exited={(resolved.kind === 'terminal' && resolved.terminal.exited) || undefined}
      data-drop={dropSide ?? undefined}
      draggable
      onDragStart={event => startDrag(event, item)}
      onDragEnd={endDrag}
      onDragOver={event => {
        if (!isDockDrag(event)) {
          return
        }
        event.preventDefault()
        event.stopPropagation()
        const r = event.currentTarget.getBoundingClientRect()
        const before = event.clientX < r.left + r.width / 2
        hint({ kind: 'tabs', group: group.id, index: before ? index : index + 1 })
      }}
      onClick={() => activateTab(item)}
      onAuxClick={event => {
        if (event.button === 1) {
          closeItem(item)
        }
      }}
      onContextMenu={(event: MouseEvent) => {
        event.preventDefault()
        menu(event.clientX, event.clientY)
      }}
      onKeyDown={onKeyDown}
    >
      <ItemIcon resolved={resolved} />
      <span className="dock-tab__title">{title}</span>
      {/* A pointer shortcut: a control inside a tab can't be reached by screen
          readers, so keyboards close tabs with Delete or the tab's menu. */}
      <span
        className="dock-tab__close"
        aria-hidden="true"
        title={`Close ${title}`}
        onMouseDown={event => event.stopPropagation()}
        onClick={event => {
          event.stopPropagation()
          closeItem(item)
        }}
      >
        <span className="dock-tab__dirty" />
        <span className="dock-tab__x">×</span>
      </span>
    </div>
  )
}

function tabMenu(item: ItemId, title: string): MenuEntry[] {
  // The commands act on the current group's active tab: this one.
  const run = (command: string) => () => {
    activateTab(item, false)
    void executeCommand(command)
  }
  return [
    { label: `Close ${title}`, run: () => closeItem(item) },
    null,
    { label: 'Split Right', run: run('view.splitRight') },
    { label: 'Split Down', run: run('view.splitDown') },
    null,
    { label: 'Move Left', run: run('view.moveTabLeft') },
    { label: 'Move Right', run: run('view.moveTabRight') },
    { label: 'Move Up', run: run('view.moveTabUp') },
    { label: 'Move Down', run: run('view.moveTabDown') },
  ]
}
