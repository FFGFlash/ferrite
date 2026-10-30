/**
 * Workbench layout, restored on the next launch. The backend stores it as
 * opaque JSON; this module owns its shape and validates what comes back.
 * The dock's part is validated by the dock (`dock/model.ts`).
 */
import { getSavedUi, saveUi } from '../ipc/workspace'
import { isDesktop } from '../ipc'
import { documentsStore } from './documents'
import { createStore } from './store'

const SAVE_DELAY_MS = 500
const MAX_EXPANDED = 200

export interface Layout {
  /** Expanded explorer folders (absolute paths). */
  expanded: string[]
  minimapVisible: boolean
  /**
   * Where everything is docked, as saved (see `dock/dock.ts`). Layouts from
   * before the dock hold `{ sidebarVisible, activeView }` here instead.
   */
  dock: unknown
}

export const DEFAULT_LAYOUT: Layout = {
  expanded: [],
  minimapVisible: true,
  dock: null,
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
  return {
    expanded: Array.isArray(saved.expanded)
      ? saved.expanded
          .filter((path): path is string => typeof path === 'string')
          .slice(0, MAX_EXPANDED)
      : [],
    minimapVisible:
      typeof saved.minimapVisible === 'boolean'
        ? saved.minimapVisible
        : DEFAULT_LAYOUT.minimapVisible,
    dock:
      saved.dock ??
      // Before the dock: the side bar's state seeds the default layout.
      (saved.sidebarVisible !== undefined || saved.activeView !== undefined
        ? { sidebarVisible: saved.sidebarVisible, activeView: saved.activeView }
        : null),
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
