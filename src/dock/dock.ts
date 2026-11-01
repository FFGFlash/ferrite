/**
 * The dock's state: where every document, view, terminal, and preview
 * shows, which group the user is working in, and the actions on them.
 *
 * The documents, terminals, and previews stores own what's open; the dock
 * follows them (adding a tab for a new document, removing it when the
 * document closes) and decides only where things go. Views are added when
 * shown.
 *
 * The layout is saved with the rest of the workbench layout. Documents are
 * saved by path (`file:<path>`), because document ids change between
 * runs; they hold their place until the documents load. Previews aren't
 * saved: their extensions open them again.
 */
import { activeEditor, editorFor } from '../editor/activeEditor'
import {
  activate as activateDocument,
  close as closeDocument,
  documentsStore,
} from '../state/documents'
import { layoutStore, updateLayout } from '../state/layout'
import { reportError } from '../state/notifications'
import {
  closePreview,
  previewsStore,
  setPreviewHandler,
  takeBesideRequest,
} from '../state/previews'
import { createStore } from '../state/store'
import { setViewHandler } from '../state/viewRequests'
import {
  closeTerminal,
  focusTerminal,
  newTerminal,
  setActiveTerminal,
  startingTerminals,
  terminalHasFocus,
  terminalsStore,
} from '../terminal/terminals'
import {
  MAIN_GROUP,
  activateItem,
  allItems,
  emptyGroup,
  findGroup,
  groupOf,
  groups,
  itemKind,
  neighbourInDirection,
  normalize,
  parseDock,
  placeItem,
  removeItem,
  resizeSplit,
  setHidden,
  sidebarGroups,
  updateGroup,
  type DockGroup,
  type DockLayout,
  type DropTarget,
  type Edge,
  type ItemId,
  type ItemKind,
} from './model'

export const documentItem = (id: number): ItemId => `document:${id}`
export const viewItem = (id: string): ItemId => `view:${id}`
export const terminalItem = (id: number): ItemId => `terminal:${id}`
export const previewItem = (id: string): ItemId => `preview:${id}`
/** A saved document, until the documents load. */
const fileItem = (path: string): ItemId => `file:${path}`

/** The id after the kind: a document or terminal number, or a view or preview id. */
export function itemId(item: ItemId): string {
  return item.slice(item.indexOf(':') + 1)
}

const BUILTIN_VIEWS = ['explorer', 'search', 'extensions'].map(viewItem)

/** Views on the left, the main group filling the rest. */
export function defaultDock(
  options: { viewsHidden?: boolean; activeView?: string } = {},
): DockLayout {
  const active = BUILTIN_VIEWS.find(item => item === viewItem(options.activeView ?? '')) ?? null
  const views: DockGroup = {
    ...emptyGroup('g1', [...BUILTIN_VIEWS]),
    ...(active ? { active } : {}),
    ...(options.viewsHidden ? { hidden: true } : {}),
  }
  return {
    main: MAIN_GROUP,
    root: {
      kind: 'split',
      id: 's1',
      direction: 'row',
      sizes: [0.2, 0.8],
      children: [views, emptyGroup(MAIN_GROUP)],
    },
  }
}

export const dockStore = createStore<DockLayout>(defaultDock())
/** The group the user last worked in. Commands such as Close Tab act on it. */
export const currentGroup = createStore<string>(MAIN_GROUP)
/** The group new documents open in, while it exists. */
let documentGroup = MAIN_GROUP
/** Where closed views were, to reopen them there. */
const closedViews = new Map<ItemId, { group: string; index: number }>()

function setLayout(layout: DockLayout): void {
  dockStore.set(layout)
}

function setCurrent(id: string): void {
  currentGroup.set(id)
  const active = findGroup(dockStore.get(), id)?.active
  if (active && itemKind(active) === 'document') {
    documentGroup = id
  }
}

/** Where a new item goes, and the share of the dock a new group for it gets. */
function placementFor(layout: DockLayout, item: ItemId): { target: DropTarget; size: number } {
  const all = groups(layout)
  const holding = (kind: 'view' | 'terminal') =>
    all.filter(group => group.items.some(other => itemKind(other) === kind))
  const appendTo = (group: DockGroup, afterActive = false): DropTarget => ({
    kind: 'tabs',
    group: group.id,
    index: afterActive && group.active ? group.items.indexOf(group.active) + 1 : group.items.length,
  })
  switch (itemKind(item)) {
    case 'document': {
      const group = findGroup(layout, documentGroup) ?? findGroup(layout, layout.main)!
      return { target: appendTo(group, true), size: 0.5 }
    }
    case 'terminal': {
      const terminals = holding('terminal')
      const group = terminals.find(g => g.id === currentGroup.get()) ?? terminals[0]
      return {
        target: group ? appendTo(group) : { kind: 'root', edge: 'bottom' },
        size: 0.3,
      }
    }
    case 'preview':
      return { target: appendTo(previewSourceGroup(layout, item), true), size: 0.5 }
    default: {
      const remembered = closedViews.get(item)
      if (remembered && findGroup(layout, remembered.group)) {
        return {
          target: { kind: 'tabs', group: remembered.group, index: remembered.index },
          size: 0.2,
        }
      }
      return { target: sidebarTarget(layout, 'view'), size: 0.2 }
    }
  }
}

/** The group of a preview's document, else the group documents open in. */
function previewSourceGroup(layout: DockLayout, item: ItemId): DockGroup {
  const document = previewsStore.get().find(preview => previewItem(preview.id) === item)?.document
  return (
    (document != null ? groupOf(layout, documentItem(document)) : undefined) ??
    findGroup(layout, documentGroup) ??
    findGroup(layout, layout.main)!
  )
}

/**
 * Beside a preview's document: in the group to the right of the document's
 * group, or in a new group there.
 */
function besideTarget(layout: DockLayout, item: ItemId): DropTarget {
  const source = previewSourceGroup(layout, item)
  const rects = visibleGroupRects()
  const from = rects.find(entry => entry.id === source.id)
  const neighbour =
    from &&
    neighbourInDirection(
      from.rect,
      'right',
      rects.filter(entry => entry.id !== source.id),
    )
  const group = neighbour && findGroup(layout, neighbour.id)
  return group
    ? { kind: 'tabs', group: group.id, index: group.items.length }
    : { kind: 'split', group: source.id, edge: 'right' }
}

/**
 * Where an item of `kind` joins the side bar: after the items of its kind
 * there, else in a new group below the side bar's others (not along the
 * left edge, which would push them out of it), else on the left.
 */
function sidebarTarget(layout: DockLayout, kind: ReturnType<typeof itemKind>): DropTarget {
  const sidebar = sidebarGroups(layout)
  const same = sidebar.find(group => group.items.some(item => itemKind(item) === kind))
  if (same) {
    return { kind: 'tabs', group: same.id, index: same.items.length }
  }
  const last = sidebar.at(-1)
  return last ? { kind: 'split', group: last.id, edge: 'bottom' } : { kind: 'root', edge: 'left' }
}

/** Whether `item` is in the side bar. */
function inSidebar(layout: DockLayout, item: ItemId): boolean {
  return sidebarGroups(layout).some(group => group.items.includes(item))
}

/**
 * What the activity bar shows: everything in the side bar (views, terminals,
 * documents), in tab order, then the views not docked anywhere (which open
 * in the side bar). `views` are the known view ids.
 */
export function activityBarItems(layout: DockLayout, views: readonly string[]): ItemId[] {
  const docked = new Set(allItems(layout))
  return [
    ...sidebarGroups(layout).flatMap(group => group.items),
    ...views.map(viewItem).filter(item => !docked.has(item)),
  ]
}

/** `layout` with `item` in it, placed by the usual rules if it wasn't. */
function ensure(layout: DockLayout, item: ItemId): DockLayout {
  if (groupOf(layout, item)) {
    return layout
  }
  const { target, size } = placementFor(layout, item)
  return placeItem(layout, item, target, size)
}

function replaceItem(layout: DockLayout, from: ItemId, to: ItemId): DockLayout {
  const group = groupOf(layout, from)
  if (!group) {
    return layout
  }
  return updateGroup(layout, group.id, g => ({
    ...g,
    items: g.items.map(item => (item === from ? to : item)),
    active: g.active === from ? to : g.active,
  }))
}

// Following the documents, terminals, and previews.

let knownDocuments = new Set<number>()
let knownTerminals = new Set<number>()
let knownPreviews = new Set<string>()

function syncDocuments(): void {
  const { documents, activeId } = documentsStore.get()
  const before = dockStore.get()
  let layout = before
  const ids = new Set(documents.map(document => document.id))
  for (const id of knownDocuments) {
    if (!ids.has(id)) {
      layout = removeItem(layout, documentItem(id))
    }
  }
  for (const document of documents) {
    const item = documentItem(document.id)
    if (groupOf(layout, item)) {
      continue
    }
    const saved = document.path === null ? null : fileItem(document.path)
    layout =
      saved && groupOf(layout, saved) ? replaceItem(layout, saved, item) : ensure(layout, item)
  }
  knownDocuments = ids
  if (activeId !== null) {
    layout = activateItem(layout, documentItem(activeId))
  }
  if (layout !== before) {
    setLayout(layout)
  }
  if (activeId !== null) {
    const group = groupOf(layout, documentItem(activeId))
    if (group) {
      setCurrent(group.id)
    }
  }
}

function syncTerminals(): void {
  const { tabs } = terminalsStore.get()
  const before = dockStore.get()
  let layout = before
  const ids = new Set(tabs.map(tab => tab.id))
  for (const id of knownTerminals) {
    if (!ids.has(id)) {
      layout = removeItem(layout, terminalItem(id))
    }
  }
  for (const tab of tabs) {
    if (!knownTerminals.has(tab.id)) {
      layout = activateItem(ensure(layout, terminalItem(tab.id)), terminalItem(tab.id))
    }
  }
  knownTerminals = ids
  if (layout !== before) {
    setLayout(layout)
  }
}

/** New previews open with their document, or beside it if their extension asked. */
function syncPreviews(): void {
  const previews = previewsStore.get()
  const before = dockStore.get()
  let layout = before
  const ids = new Set(previews.map(preview => preview.id))
  for (const id of knownPreviews) {
    if (!ids.has(id)) {
      layout = removeItem(layout, previewItem(id))
    }
  }
  for (const preview of previews) {
    const item = previewItem(preview.id)
    if (groupOf(layout, item)) {
      continue
    }
    layout = takeBesideRequest(preview.id)
      ? placeItem(layout, item, besideTarget(layout, item), 0.5)
      : ensure(layout, item)
  }
  knownPreviews = ids
  if (layout !== before) {
    setLayout(layout)
  }
}

documentsStore.subscribe(syncDocuments)
terminalsStore.subscribe(syncTerminals)
previewsStore.subscribe(syncPreviews)

// Saving and restoring.

/**
 * The layout as saved: documents by path; untitled documents and previews
 * aren't saved.
 */
export function serializeDock(layout: DockLayout = dockStore.get()): DockLayout {
  const paths = new Map(
    documentsStore.get().documents.map(document => [documentItem(document.id), document.path]),
  )
  const saved = (item: ItemId): ItemId | null => {
    if (itemKind(item) === 'preview') {
      return null
    }
    if (itemKind(item) !== 'document') {
      return item
    }
    const path = paths.get(item)
    return path ? fileItem(path) : null
  }
  let result = layout
  for (const group of groups(layout)) {
    result = updateGroup(result, group.id, g => {
      const items = g.items.map(saved).filter((item): item is ItemId => item !== null)
      const active = g.active === null ? null : saved(g.active)
      return { ...g, items, active: active ?? items[0] ?? null }
    })
  }
  return result
}

const SAVED_ITEM = /^(file:.+|view:.+|terminal:\d+)$/s

/**
 * Restores the saved layout (or the default, honouring the side bar
 * settings of older versions). Call before the documents and terminals load.
 */
export function restoreDock(): void {
  const saved = layoutStore.get().dock
  const legacy = (typeof saved === 'object' && saved !== null ? saved : {}) as Record<
    string,
    unknown
  >
  const layout =
    parseDock(saved, item => SAVED_ITEM.test(item)) ??
    defaultDock({
      viewsHidden: legacy.sidebarVisible === false,
      activeView: typeof legacy.activeView === 'string' ? legacy.activeView : undefined,
    })
  knownDocuments = new Set()
  knownTerminals = new Set()
  knownPreviews = new Set()
  setLayout(layout)
  currentGroup.set(layout.main)
  documentGroup = layout.main
  syncDocuments()
  syncTerminals()
  syncPreviews()
}

/** Drops saved documents that didn't open again. */
export function documentsRestored(): void {
  const layout = dockStore.get()
  const stale = allItems(layout).filter(item => item.startsWith('file:'))
  if (stale.length > 0) {
    setLayout(stale.reduce(removeItem, layout))
  }
}

/**
 * After reattaching to the running terminals: if the saved layout showed
 * terminals but none is running (a new start of the app), starts one where
 * they were. Then drops the saved terminals that are gone.
 */
export function terminalsRestored(): Promise<void> {
  // Restoring twice (React runs effects twice in development) shares one run.
  restoringTerminals ??= restoreTerminalSlots().finally(() => {
    restoringTerminals = null
  })
  return restoringTerminals
}

let restoringTerminals: Promise<void> | null = null

async function restoreTerminalSlots(): Promise<void> {
  const running = new Set(terminalsStore.get().tabs.map(tab => terminalItem(tab.id)))
  const shown = groups(dockStore.get()).some(
    group => !group.hidden && group.items.some(item => itemKind(item) === 'terminal'),
  )
  if (running.size === 0 && shown && startingTerminals.get() === 0) {
    try {
      await newTerminal()
    } catch (error) {
      reportError('Starting a terminal', error)
    }
  }
  const alive = new Set(terminalsStore.get().tabs.map(tab => terminalItem(tab.id)))
  const layout = dockStore.get()
  const stale = allItems(layout).filter(item => itemKind(item) === 'terminal' && !alive.has(item))
  if (stale.length > 0) {
    setLayout(stale.reduce(removeItem, layout))
  }
}

// Persist with the rest of the layout, after it has been restored.
dockStore.subscribe(() => updateLayout({ dock: serializeDock() }))

// Showing, hiding, and moving.

/** Focuses an item once it's rendered: the editor, the terminal, or the view's first control. */
export function focusItem(item: ItemId): void {
  const id = itemId(item)
  switch (itemKind(item)) {
    case 'terminal':
      focusTerminal(Number(id))
      return
    case 'document':
      requestAnimationFrame(() => (editorFor(Number(id)) ?? activeEditor.get())?.focus())
      return
    default:
      requestAnimationFrame(() => {
        const group = groupOf(dockStore.get(), item)
        const content = group && contentElement(group.id)
        const control = content?.querySelector<HTMLElement>(
          'input, textarea, [role="tree"], [tabindex="0"], button',
        )
        ;(control ?? content)?.focus()
      })
  }
}

function contentElement(group: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-group-content="${CSS.escape(group)}"]`)
}

function groupElement(group: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-group="${CSS.escape(group)}"]`)
}

/** Whether keyboard focus is inside the group `id`. */
function hasFocus(id: string): boolean {
  return groupElement(id)?.contains(document.activeElement) ?? false
}

/** Shows `item` (adding it if needed), makes its group current, and optionally focuses it. */
export function showItem(item: ItemId, focus = true): void {
  setLayout(activateItem(ensure(dockStore.get(), item), item))
  const group = groupOf(dockStore.get(), item)
  if (group) {
    setCurrent(group.id)
  }
  noteActive(item)
  if (focus) {
    focusItem(item)
  }
}

export function showView(id: string, focus = true): void {
  showItem(viewItem(id), focus)
}

setViewHandler(showView)

/**
 * Shows a preview its extension asked to show. One opened beside its
 * document leaves focus (and the current group) in the editor, so typing
 * carries on and updates it.
 */
function showPreview(id: string, beside: boolean): void {
  const item = previewItem(id)
  if (beside) {
    setLayout(activateItem(ensure(dockStore.get(), item), item))
  } else {
    showItem(item)
  }
}

setPreviewHandler(showPreview)

/** Records the item as the one in use, for the stores that care. */
function noteActive(item: ItemId): void {
  const id = itemId(item)
  if (itemKind(item) === 'document' && documentsStore.get().activeId !== Number(id)) {
    activateDocument(Number(id))
  } else if (itemKind(item) === 'terminal') {
    setActiveTerminal(Number(id))
  }
}

/** A tab was chosen: shows and focuses it. */
export function activateTab(item: ItemId, focus = true): void {
  showItem(item, focus)
}

/** Focus moved into a group: it becomes the current group. */
export function groupFocused(id: string): void {
  if (currentGroup.get() === id) {
    return
  }
  setCurrent(id)
  const active = findGroup(dockStore.get(), id)?.active
  if (active) {
    noteActive(active)
  }
}

/**
 * The activity bar's buttons: shows an item, or, when it's already the
 * shown tab of its group, hides that group (as collapsing a side bar does).
 * A view that isn't docked opens in the side bar, where its button is.
 */
export function toggleItem(item: ItemId): void {
  const layout = dockStore.get()
  const group = groupOf(layout, item)
  if (group && !group.hidden && group.active === item) {
    hideGroups([group.id])
    return
  }
  if (!group) {
    const remembered = closedViews.get(item)
    const target =
      remembered && sidebarGroups(layout).some(g => g.id === remembered.group)
        ? ({ kind: 'tabs', group: remembered.group, index: remembered.index } as const)
        : sidebarTarget(layout, 'view')
    setLayout(placeItem(layout, item, target, 0.2))
  }
  showItem(item)
}

/** An item dropped on the activity bar: moves it into the side bar. */
export function dockInSidebar(item: ItemId): void {
  const layout = dockStore.get()
  if (inSidebar(layout, item)) {
    showItem(item)
  } else {
    moveItem(item, sidebarTarget(layout, itemKind(item)))
    focusItem(item)
  }
}

/** Hides groups, moving focus to the editor if it was in one of them. */
function hideGroups(ids: readonly string[]): void {
  const hadFocus = ids.some(hasFocus)
  setLayout(ids.reduce((layout, id) => setHidden(layout, id, true), dockStore.get()))
  if (hadFocus) {
    activeEditor.get()?.focus()
  }
}

function showGroups(ids: readonly string[]): void {
  setLayout(ids.reduce((layout, id) => setHidden(layout, id, false), dockStore.get()))
}

/** Groups holding only items of `kind` (never the main group). */
function groupsOfKind(kind: 'terminal'): DockGroup[] {
  const layout = dockStore.get()
  return groups(layout).filter(
    group =>
      group.id !== layout.main &&
      group.items.length > 0 &&
      group.items.every(item => itemKind(item) === kind),
  )
}

/**
 * Hides the side bar, or shows it again (View: Toggle Side Bar). With
 * nothing in it, opens the first built-in view not docked elsewhere there.
 */
export function toggleSidebar(): void {
  const layout = dockStore.get()
  const sidebar = sidebarGroups(layout).map(group => group.id)
  if (sidebar.some(id => !findGroup(layout, id)!.hidden)) {
    hideGroups(sidebar)
  } else if (sidebar.length > 0) {
    showGroups(sidebar)
  } else {
    const undocked = BUILTIN_VIEWS.find(item => !groupOf(layout, item))
    if (undocked) {
      setLayout(placeItem(layout, undocked, sidebarTarget(layout, 'view'), 0.2))
      showItem(undocked, false)
    }
  }
}

/** The terminal to show: the last used one, else any. */
function preferredTerminal(): ItemId | null {
  const { tabs, activeId } = terminalsStore.get()
  const id = activeId ?? tabs.at(-1)?.id
  return id === undefined ? null : terminalItem(id)
}

/** Starts a terminal with a profile (the default if `null`) and shows it. */
export async function openTerminal(profile: string | null = null): Promise<void> {
  const id = await newTerminal(profile)
  showItem(terminalItem(id))
}

/** Shows and focuses a terminal, starting one if none is open. */
export async function focusActiveTerminal(): Promise<void> {
  const item = preferredTerminal()
  if (item) {
    showGroups(groupsOfKind('terminal').map(group => group.id))
    showItem(item)
  } else {
    await openTerminal()
  }
}

/**
 * Shows and focuses the terminal; from a terminal, hides the terminals and
 * returns to the editor.
 */
export async function toggleTerminal(): Promise<void> {
  if (terminalHasFocus()) {
    const groupsToHide = groupsOfKind('terminal').map(group => group.id)
    if (groupsToHide.length > 0) {
      hideGroups(groupsToHide)
    } else {
      activeEditor.get()?.focus()
    }
  } else {
    await focusActiveTerminal()
  }
}

/** Hides every group of terminals, or shows them (starting a terminal if there's none). */
export async function toggleTerminals(): Promise<void> {
  const terminalGroups = groupsOfKind('terminal')
  if (terminalGroups.some(group => !group.hidden)) {
    hideGroups(terminalGroups.map(group => group.id))
  } else {
    await focusActiveTerminal()
  }
}

/** Closes a tab: closes the document, ends the terminal, or closes the view or preview. */
export function closeItem(item: ItemId): void {
  const group = groupOf(dockStore.get(), item)
  const hadFocus = group ? hasFocus(group.id) : false
  const id = itemId(item)
  switch (itemKind(item)) {
    case 'document':
      void closeDocument(Number(id))
      return
    case 'terminal':
      closeTerminal(Number(id))
      break
    case 'preview':
      closePreview(id)
      setLayout(removeItem(dockStore.get(), item))
      break
    default:
      if (group) {
        closedViews.set(item, { group: group.id, index: group.items.indexOf(item) })
      }
      setLayout(removeItem(dockStore.get(), item))
  }
  // Keep focus in the group, or go back to the editor.
  const next = group && findGroup(dockStore.get(), group.id)?.active
  if (hadFocus) {
    if (next) {
      showItem(next)
    } else {
      activeEditor.get()?.focus()
    }
  }
}

/** The current group, if it still exists. */
function current(): DockGroup | undefined {
  return findGroup(dockStore.get(), currentGroup.get())
}

export function hasCurrentTab(): boolean {
  return current()?.active != null
}

export function closeCurrentTab(): void {
  const active = current()?.active
  if (active) {
    closeItem(active)
  }
}

/** Activates the tab `step` places from the current one, in the current group. */
export function cycleTab(step: number): void {
  const group = current()
  if (!group || group.items.length === 0) {
    return
  }
  const index = group.active ? group.items.indexOf(group.active) : 0
  const next = group.items[(index + step + group.items.length) % group.items.length]!
  showItem(next)
}

/** Moves an item (dragged, or from a command) and makes it current. */
export function moveItem(item: ItemId, target: DropTarget): void {
  const layout = dockStore.get()
  const kind = itemKind(item)
  const next = placeItem(layout, item, target, kind === 'terminal' ? 0.3 : 0.2)
  if (next === layout) {
    return
  }
  setLayout(next)
  const group = groupOf(next, item)
  if (group) {
    setCurrent(group.id)
  }
  noteActive(item)
}

/** The groups on screen, with their rectangles. */
function visibleGroupRects(): { id: string; rect: DOMRect }[] {
  return [...document.querySelectorAll<HTMLElement>('[data-group]')].map(element => ({
    id: element.dataset.group!,
    rect: element.getBoundingClientRect(),
  }))
}

/**
 * Moves the current tab into the neighbouring group on one side, or into a
 * new group on that side when there is none.
 */
export function moveCurrentTab(edge: Edge): void {
  const group = current()
  const item = group?.active
  if (!group || !item) {
    return
  }
  const rects = visibleGroupRects()
  const from = rects.find(entry => entry.id === group.id)
  const neighbour =
    from &&
    neighbourInDirection(
      from.rect,
      edge,
      rects.filter(entry => entry.id !== group.id),
    )
  if (neighbour) {
    const target = findGroup(dockStore.get(), neighbour.id)
    moveItem(item, { kind: 'tabs', group: neighbour.id, index: target?.items.length ?? 0 })
  } else {
    moveItem(item, { kind: 'split', group: group.id, edge })
  }
  focusItem(item)
}

/** Moves the current tab into a new group beside its group. */
export function splitCurrentTab(edge: Edge): void {
  const group = current()
  if (group?.active) {
    moveItem(group.active, { kind: 'split', group: group.id, edge })
    focusItem(group.active)
  }
}

/** Focuses the next (or previous) visible group, in reading order. */
export function focusGroupRelative(step: number): void {
  const rects = visibleGroupRects()
  if (rects.length === 0) {
    return
  }
  const index = rects.findIndex(entry => entry.id === currentGroup.get())
  const next = rects[(index + step + rects.length) % rects.length]!
  const group = findGroup(dockStore.get(), next.id)
  setCurrent(next.id)
  if (group?.active) {
    showItem(group.active)
  } else {
    contentElement(next.id)?.focus()
  }
}

export function resize(splitId: string, sizes: number[]): void {
  setLayout(resizeSplit(dockStore.get(), splitId, sizes))
}

/**
 * Puts everything back where it starts out: views on the left, documents in
 * the main group, terminals along the bottom.
 */
export function resetLayout(): void {
  const old = dockStore.get()
  const items = allItems(old)
  let layout = defaultDock()
  const placeAll = (kind: ItemKind) => {
    for (const item of items.filter(other => itemKind(other) === kind)) {
      if (!groupOf(layout, item)) {
        const { target, size } = placementFor(layout, item)
        layout = placeItem(layout, item, target, size)
      }
    }
  }
  closedViews.clear()
  documentGroup = MAIN_GROUP
  currentGroup.set(MAIN_GROUP)
  placeAll('view')
  placeAll('document')
  placeAll('preview')
  placeAll('terminal')
  setLayout(normalize(layout))
  const { activeId } = documentsStore.get()
  if (activeId !== null) {
    showItem(documentItem(activeId), false)
  }
}
