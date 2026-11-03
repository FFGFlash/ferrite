/**
 * Extensions' side bar views: their content (fetched once per change, for
 * both a view's header and its items), their badges, the containers whose
 * sections they are, and the menus contributed for them.
 */
import { useEffect, useMemo, useState } from 'react'
import {
  getViewContent,
  listViewBadges,
  type MenuContribution,
  type ViewContent,
  type ViewContribution,
} from '../ipc/extensions'
import { extensionsStore, usableExtensions, viewVersions } from './extensions'
import { reportError } from './notifications'
import { createStore, useStore } from './store'

/** Views' badges, by full view id. */
export const viewBadges = createStore<Readonly<Record<string, number>>>({})

let badgesLoading: Promise<void> | null = null
let badgesAgain = false

/** Reloads views' badges; coalesces calls made while one is running. */
export function refreshViewBadges(): Promise<void> {
  if (badgesLoading) {
    badgesAgain = true
    return badgesLoading
  }
  badgesLoading = (async () => {
    do {
      badgesAgain = false
      try {
        viewBadges.set(await listViewBadges())
      } catch (error) {
        reportError('Loading view badges', error)
      }
    } while (badgesAgain)
    badgesLoading = null
  })()
  return badgesLoading
}

/** Content fetched for each view, by version, shared by its header and its items. */
const fetched = new Map<string, { version: number; content: Promise<ViewContent> }>()

/** A view's content, fetched again when the extension changes it. */
export function useViewContent(view: string): ViewContent | null {
  const version = useStore(viewVersions)[view] ?? 0
  const [content, setContent] = useState<ViewContent | null>(null)
  useEffect(() => {
    let cached = fetched.get(view)
    if (!cached || cached.version !== version) {
      cached = { version, content: getViewContent(view) }
      fetched.set(view, cached)
    }
    let cancelled = false
    cached.content
      .then(next => {
        if (!cancelled) {
          setContent(next)
        }
      })
      .catch((error: unknown) => {
        fetched.delete(view)
        reportError('Loading the view', error)
      })
    return () => {
      cancelled = true
    }
  }, [view, version])
  return content
}

/** Every view extensions contribute, by full id. */
export function useContributedViews(): ReadonlyMap<string, ViewContribution> {
  const extensions = useStore(extensionsStore)
  return useMemo(
    () =>
      new Map(
        usableExtensions(extensions).flatMap(extension =>
          extension.contributions.views.map(view => [view.fullId, view] as const),
        ),
      ),
    [extensions],
  )
}

/** The views shown as sections of `container`, in their order. */
export function sectionsOf(container: string, extensions = extensionsStore.get()): string[] {
  return usableExtensions(extensions).flatMap(extension =>
    extension.contributions.views
      .filter(view => view.container === container)
      .map(view => view.fullId),
  )
}

/** The container a view is a section of, if it's one. */
export function containerOf(view: string, extensions = extensionsStore.get()): string | null {
  for (const extension of usableExtensions(extensions)) {
    const found = extension.contributions.views.find(candidate => candidate.fullId === view)
    if (found) {
      return found.container
    }
  }
  return null
}

/** A view menu entry, with its command's title. */
export interface ViewMenuItem extends MenuContribution {
  title: string
}

/** The menus contributed for `view` at `location`, sorted by group and title. */
export function viewMenus(
  view: string,
  location: 'view/title' | 'view/item',
  extensions = extensionsStore.get(),
): ViewMenuItem[] {
  const items: ViewMenuItem[] = []
  for (const extension of usableExtensions(extensions)) {
    for (const menu of extension.contributions.menus) {
      const command = extension.commands.find(candidate => candidate.id === menu.command)
      if (menu.location === location && menu.view === view && command) {
        items.push({ ...menu, title: command.title })
      }
    }
  }
  return items.sort((a, b) => a.group.localeCompare(b.group) || a.title.localeCompare(b.title))
}

/** Whether a menu's `when` holds for a view's flags. */
export function whenHolds(when: string | null, flags: readonly string[]): boolean {
  if (when === null) {
    return true
  }
  return when.startsWith('!') ? !flags.includes(when.slice(1)) : flags.includes(when)
}

/** Whether an item menu is for an item with `context`. */
export function forContext(menu: MenuContribution, context: string | null): boolean {
  return menu.contexts.length === 0 || (context !== null && menu.contexts.includes(context))
}
