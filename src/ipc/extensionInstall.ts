/**
 * Typed wrappers for installing extensions from GitHub releases
 * (`src-tauri/src/extension_install.rs`).
 */
import { call } from './index'

export interface ExtensionRelease {
  tag: string
  /** Its title, if it has one besides the tag. */
  name: string | null
  prerelease: boolean
  /** ISO 8601. */
  publishedAt: string | null
  /** The release's page. */
  url: string
  /** Whether it has an `extension.zip` to install. */
  installable: boolean
}

export interface ExtensionReleases {
  /** `owner/name`. */
  repository: string
  releases: ExtensionRelease[]
}

export interface InstalledExtension {
  id: string
  name: string
  version: string
}

/** An extension an owner in `extensions.catalog` lists. */
export interface CatalogEntry {
  /** `owner/name`. */
  repo: string
  id: string
  name: string
  description: string
  author: string | null
  version: string
  /** The release it's from. */
  tag: string
}

/** Where an installed extension came from, and its update, if one was found. */
export interface ExtensionSource {
  id: string
  repository: string
  tag: string
  /** The newer release's tag, from the last check. */
  update: string | null
}

export interface ExtensionUpdate {
  id: string
  repository: string
  /** The installed version. */
  version: string
  /** The newer release's tag. */
  tag: string
}

/** A repository's releases, newest first. `repository` is `owner/name` or a GitHub address. */
export const listExtensionReleases = (repository: string) =>
  call<ExtensionReleases>('extension_releases', { repository })
/**
 * Downloads a release (`tag`, or the newest), asks, and installs it.
 * `null` if the user declined.
 */
export const installExtension = (repository: string, tag: string | null = null) =>
  call<InstalledExtension | null>('extension_install', { repository, tag })
/** Asks, then uninstalls an installed extension. False if the user declined. */
export const uninstallExtension = (id: string) => call<boolean>('extension_uninstall', { id })
/** The catalog, fetched once a session unless `refresh`. */
export const getExtensionCatalog = (refresh = false) =>
  call<CatalogEntry[]>('extension_catalog', { refresh })
export const listExtensionSources = () => call<ExtensionSource[]>('extension_sources')
export const checkExtensionUpdates = () => call<ExtensionUpdate[]>('extension_check_updates')
