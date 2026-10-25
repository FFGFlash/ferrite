/**
 * Workbench layout, restored on the next launch. The backend stores it as
 * opaque JSON; this module owns its shape and validates what comes back.
 */
import { getSavedUi, saveUi } from '../ipc/workspace'
import { isDesktop } from '../ipc'
import { documentsStore } from './documents'
import { createStore } from './store'

export const SIDEBAR_DEFAULT_WIDTH = 260
export const SIDEBAR_MIN_WIDTH = 170
export const SIDEBAR_MAX_WIDTH = 600
const SAVE_DELAY_MS = 500
const MAX_EXPANDED = 200

export interface Layout {
  sidebarVisible: boolean
  sidebarWidth: number
  activeView: string
  /** Expanded explorer folders (absolute paths). */
  expanded: string[]
  minimapVisible: boolean
}

export const DEFAULT_LAYOUT: Layout = {
  sidebarVisible: true,
  sidebarWidth: SIDEBAR_DEFAULT_WIDTH,
  activeView: 'explorer',
  expanded: [],
  minimapVisible: true,
}

export const layoutStore = createStore<Layout>(DEFAULT_LAYOUT)

export function updateLayout(change: Partial<Layout>): void {
  layoutStore.update(current => ({ ...current, ...change }))
}

/** Validates a saved layout, falling back to defaults field by field. */
export function parseLayout(input: unknown): Layout {
  const saved: Record<string, unknown> =
    typeof input === 'object' && input !== null && !Array.isArray(input)
      ? (input as Record<string, unknown>)
      : {}
  const width = Number(saved.sidebarWidth)
  return {
    sidebarVisible:
      typeof saved.sidebarVisible === 'boolean'
        ? saved.sidebarVisible
        : DEFAULT_LAYOUT.sidebarVisible,
    sidebarWidth: Number.isFinite(width)
      ? Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, width))
      : DEFAULT_LAYOUT.sidebarWidth,
    activeView: typeof saved.activeView === 'string' ? saved.activeView : DEFAULT_LAYOUT.activeView,
    expanded: Array.isArray(saved.expanded)
      ? saved.expanded
          .filter((path): path is string => typeof path === 'string')
          .slice(0, MAX_EXPANDED)
      : [],
    minimapVisible:
      typeof saved.minimapVisible === 'boolean'
        ? saved.minimapVisible
        : DEFAULT_LAYOUT.minimapVisible,
  }
}

/**
 * Loads the saved layout, then saves changes to it (and to the active file)
 * after they settle. Returns the saved active file path, if any.
 */
export async function restoreLayout(): Promise<string | null> {
  if (!isDesktop()) {
    return null
  }
  const saved = await getSavedUi()
  layoutStore.set(parseLayout(saved.layout))

  let timer: ReturnType<typeof setTimeout> | undefined
  const scheduleSave = () => {
    clearTimeout(timer)
    timer = setTimeout(() => {
      const { documents, activeId } = documentsStore.get()
      const active = documents.find(document => document.id === activeId)?.path ?? null
      const layout = layoutStore.get()
      void saveUi({ ...layout, expanded: layout.expanded.slice(0, MAX_EXPANDED) }, active).catch(
        () => {},
      )
    }, SAVE_DELAY_MS)
  }
  layoutStore.subscribe(scheduleSave)
  documentsStore.subscribe(scheduleSave)
  return saved.activeFile
}
