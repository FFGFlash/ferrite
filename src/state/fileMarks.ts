/**
 * Marks extensions put on files and folders in the explorer, such as a
 * version control status: a color, a badge, and a tooltip. A mark can cover
 * everything inside a folder (`inherit`), and color the folders containing
 * it (`propagate`). Colors are extensions' theme tokens, resolved for the
 * theme in effect.
 */
import { useMemo } from 'react'
import { listFileMarks, type ExtensionMarks, type FileMark } from '../ipc/extensions'
import { extensionColorOverrides, themeKind, type ThemeKind } from '../themes/theme'
import { extensionsStore, usableExtensions } from './extensions'
import { reportError } from './notifications'
import { createStore, useStore } from './store'

export const fileMarksStore = createStore<readonly ExtensionMarks[]>([])

let refreshing: Promise<void> | null = null
let refreshAgain = false

/** Loads every extension's marks; coalesces calls made while one is running. */
export function refreshFileMarks(): Promise<void> {
  if (refreshing) {
    refreshAgain = true
    return refreshing
  }
  refreshing = (async () => {
    do {
      refreshAgain = false
      try {
        fileMarksStore.set(await listFileMarks())
      } catch (error) {
        reportError('Loading explorer marks', error)
      }
    } while (refreshAgain)
    refreshing = null
  })()
  return refreshing
}

/** What a row of the explorer shows for marks. */
export interface RowMark {
  /** Badges of every mark on it, such as `M`, or a dot on a folder whose contents are marked. */
  badge: string | null
  /** A resolved hex color. */
  color: string | null
  tooltip: string | null
  faded: boolean
}

/** Marks arranged for looking up rows. */
export interface MarkIndex {
  /** Marks on each path, of every extension. */
  exact: ReadonlyMap<string, readonly FileMark[]>
  /** Folders whose marks cover everything inside them. */
  inherited: ReadonlyMap<string, readonly FileMark[]>
  /** For each folder, the highest-priority mark propagated to it. */
  propagated: ReadonlyMap<string, FileMark>
}

const parent = (path: string) => {
  const separator = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'))
  return separator > 0 ? path.slice(0, separator) : ''
}

/** Indexes extensions' marks (in the order given, which breaks ties). */
export function indexMarks(all: readonly ExtensionMarks[]): MarkIndex {
  const exact = new Map<string, FileMark[]>()
  const inherited = new Map<string, FileMark[]>()
  const propagated = new Map<string, FileMark>()
  for (const { marks } of all) {
    for (const mark of marks) {
      exact.set(mark.path, [...(exact.get(mark.path) ?? []), mark])
      if (mark.inherit) {
        inherited.set(mark.path, [...(inherited.get(mark.path) ?? []), mark])
      }
      if (!mark.propagate || mark.color === null) {
        continue
      }
      for (let folder = parent(mark.path); folder; folder = parent(folder)) {
        const current = propagated.get(folder)
        if (current && current.priority >= mark.priority) {
          // Its folders have a mark at least as strong, from this or another path.
          break
        }
        propagated.set(folder, mark)
      }
    }
  }
  return { exact, inherited, propagated }
}

/** Colors by token, for the theme in effect: the user's, the theme's, or the extension's default. */
export type ColorResolver = (token: string) => string | null

export function colorResolver(
  declared: ReadonlyMap<string, { dark: string; light: string }>,
  overrides: Readonly<Record<string, string>>,
  kind: ThemeKind,
): ColorResolver {
  return token => overrides[token] ?? declared.get(token)?.[kind] ?? null
}

/** The marks shown on a row: its own, or those of a folder it's inside that cover it. */
function marksOn(index: MarkIndex, path: string): readonly FileMark[] {
  const own = index.exact.get(path)
  if (own) {
    return own
  }
  if (index.inherited.size > 0) {
    for (let folder = parent(path); folder; folder = parent(folder)) {
      const covering = index.inherited.get(folder)
      if (covering) {
        return covering
      }
    }
  }
  return []
}

/** What the explorer shows for `path`; `null` without marks. */
export function rowMark(
  index: MarkIndex,
  path: string,
  directory: boolean,
  color: ColorResolver,
): RowMark | null {
  const marks = marksOn(index, path)
  if (marks.length > 0) {
    const strongest = [...marks]
      .filter(mark => mark.color !== null)
      .sort((a, b) => b.priority - a.priority)[0]
    const badges = marks.flatMap(mark => (mark.badge ? [mark.badge] : []))
    const tooltips = marks.flatMap(mark => (mark.tooltip ? [mark.tooltip] : []))
    return {
      badge: badges.length > 0 ? badges.join(', ') : null,
      color: strongest?.color ? color(strongest.color) : null,
      tooltip: tooltips.length > 0 ? tooltips.join('\n') : null,
      faded: marks.some(mark => mark.faded),
    }
  }
  const inside = directory ? index.propagated.get(path) : undefined
  if (inside?.color) {
    return {
      badge: '●',
      color: color(inside.color),
      tooltip: inside.tooltip ? `Contains: ${inside.tooltip}` : null,
      faded: false,
    }
  }
  return null
}

/** The marks index and color resolver, following marks, extensions, and the theme. */
export function useFileMarks(): { index: MarkIndex; color: ColorResolver } {
  const marks = useStore(fileMarksStore)
  const index = useMemo(() => indexMarks(marks), [marks])
  return { index, color: useColorResolver() }
}

/** Extensions' colors for the theme in effect. */
export function useColorResolver(): ColorResolver {
  const extensions = useStore(extensionsStore)
  const overrides = useStore(extensionColorOverrides)
  const kind = useStore(themeKind)
  return useMemo(() => {
    const declared = new Map(
      usableExtensions(extensions).flatMap(extension =>
        extension.contributions.colors.map(color => [color.id, color] as const),
      ),
    )
    return colorResolver(declared, overrides, kind)
  }, [extensions, overrides, kind])
}
