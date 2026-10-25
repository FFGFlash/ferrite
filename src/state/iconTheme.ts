/**
 * The active file icon theme: the built-in icons (`default`), or a theme
 * contributed by an extension, fetched from the backend when chosen. Its
 * associations are kept in maps, so a name like `constructor` can't reach
 * an object's prototype.
 */
import { getIconTheme, type IconAssociations, type IconTheme } from '../ipc/extensions'
import { extensionsStore, usableExtensions } from './extensions'
import { reportError } from './notifications'
import { createStore } from './store'

export const DEFAULT_ICON_THEME = 'default'

export interface Associations {
  file: string | null
  folder: string | null
  folderExpanded: string | null
  fileExtensions: ReadonlyMap<string, string>
  fileNames: ReadonlyMap<string, string>
  folderNames: ReadonlyMap<string, string>
  folderNamesExpanded: ReadonlyMap<string, string>
}

export interface LoadedIconTheme {
  id: string
  /** Icon id to a `data:` URL. */
  icons: ReadonlyMap<string, string>
  associations: Associations
  /** Looked up before `associations` with a light color theme. */
  light: Associations
}

/** The contributed theme in use, or `null` for the built-in icons. */
export const iconThemeStore = createStore<LoadedIconTheme | null>(null)

let wanted = DEFAULT_ICON_THEME
let request = 0

/** Uses the icon theme `id`, once an enabled extension provides it. */
export function setActiveIconTheme(id: string): void {
  if (id !== wanted) {
    wanted = id
    void load()
  }
}

async function load(): Promise<void> {
  const ticket = ++request
  const id = wanted
  const contributed =
    id !== DEFAULT_ICON_THEME &&
    usableExtensions(extensionsStore.get()).some(extension =>
      extension.contributions.iconThemes.some(theme => theme.id === id),
    )
  if (!contributed) {
    // Built in, or its extension isn't loaded (yet): this runs again when
    // extensions change.
    iconThemeStore.set(null)
    return
  }
  try {
    const theme = await getIconTheme(id)
    if (ticket === request) {
      iconThemeStore.set(theme ? prepare(id, theme) : null)
    }
  } catch (error) {
    if (ticket === request) {
      iconThemeStore.set(null)
      reportError('Loading the file icon theme', error)
    }
  }
}

function prepare(id: string, theme: IconTheme): LoadedIconTheme {
  return {
    id,
    icons: new Map(Object.entries(theme.icons)),
    associations: associations(theme.associations),
    light: associations(theme.light),
  }
}

function associations(raw: IconAssociations): Associations {
  return {
    file: raw.file,
    folder: raw.folder,
    folderExpanded: raw.folderExpanded,
    fileExtensions: new Map(Object.entries(raw.fileExtensions)),
    fileNames: new Map(Object.entries(raw.fileNames)),
    folderNames: new Map(Object.entries(raw.folderNames)),
    folderNamesExpanded: new Map(Object.entries(raw.folderNamesExpanded)),
  }
}

// The theme's extension may load, reload with new icons, or be turned off
// after the setting is applied.
extensionsStore.subscribe(() => {
  if (wanted !== DEFAULT_ICON_THEME) {
    void load()
  }
})
