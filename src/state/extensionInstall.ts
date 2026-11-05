/**
 * Installing extensions from GitHub releases: by repository, from the
 * catalog of official extensions, updating, and uninstalling. The backend
 * downloads and checks a package, then asks before installing it; these
 * flows choose what to install and say how it went.
 */
import { openQuickPick } from '../components/paletteState'
import {
  checkExtensionUpdates,
  getExtensionCatalog,
  installExtension,
  listExtensionReleases,
  listExtensionSources,
  uninstallExtension,
  type CatalogEntry,
  type ExtensionRelease,
  type ExtensionSource,
} from '../ipc/extensionInstall'
import type { ExtensionStatus } from '../ipc/extensions'
import { openExtension } from './extensions'
import { notify, reportError } from './notifications'
import { createStore } from './store'
import { showView } from './viewRequests'

/** Where each extension installed from GitHub came from, by id. */
export const sourcesStore = createStore<Readonly<Record<string, ExtensionSource>>>({})

export type CatalogState =
  | { state: 'idle' }
  | { state: 'loading' }
  | { state: 'loaded'; entries: readonly CatalogEntry[] }
  | { state: 'failed'; message: string }

export const catalogStore = createStore<CatalogState>({ state: 'idle' })

/** Whether an install or uninstall is running, to disable the buttons that start one. */
export const installBusy = createStore<boolean>(false)

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

export async function refreshSources(): Promise<void> {
  try {
    const sources = await listExtensionSources()
    sourcesStore.set(Object.fromEntries(sources.map(source => [source.id, source])))
  } catch (error) {
    reportError('Reading where extensions came from', error)
  }
}

/** Loads the catalog (from the backend's cache unless `refresh`). */
export async function loadCatalog(refresh = false): Promise<void> {
  const current = catalogStore.get()
  if (current.state === 'loading' || (current.state === 'loaded' && !refresh)) {
    return
  }
  catalogStore.set({ state: 'loading' })
  try {
    catalogStore.set({ state: 'loaded', entries: await getExtensionCatalog(refresh) })
  } catch (error) {
    catalogStore.set({ state: 'failed', message: message(error) })
  }
}

/** Installs a release (`tag`, or the newest) and opens the extension's page. */
export async function installRelease(repository: string, tag: string | null = null): Promise<void> {
  if (installBusy.get()) {
    return
  }
  installBusy.set(true)
  try {
    const installed = await installExtension(repository, tag)
    if (installed) {
      notify('info', `Installed ${installed.name} ${installed.version}.`)
      await refreshSources()
      openExtension(installed.id)
    }
  } catch (error) {
    reportError(`Installing from ${repository}`, error)
  } finally {
    installBusy.set(false)
  }
}

/** A release's line in a list of them. */
function releaseItem(
  release: ExtensionRelease,
  newest: boolean,
): { label: string; detail: string } {
  const label = release.name ? `${release.tag} — ${release.name}` : release.tag
  const details = [
    newest ? 'Latest' : null,
    release.prerelease ? 'Pre-release' : null,
    release.publishedAt ? release.publishedAt.slice(0, 10) : null,
  ]
  return { label, detail: details.filter(Boolean).join(' · ') }
}

/** Lists a repository's releases to choose one to install. */
export async function chooseRelease(repository: string): Promise<void> {
  let found
  try {
    found = await listExtensionReleases(repository)
  } catch (error) {
    reportError(`Listing the releases of ${repository}`, error)
    return
  }
  const releases = found.releases.filter(release => release.installable)
  if (releases.length === 0) {
    notify('warning', `${found.repository} has no releases with an extension.zip to install.`)
    return
  }
  const newest = releases.find(release => !release.prerelease)
  openQuickPick({
    placeholder: `Choose a release of ${found.repository} to install`,
    items: releases.map(release => ({
      ...releaseItem(release, release === newest),
      run: () => void installRelease(found.repository, release.tag),
    })),
  })
}

/** Asks for a repository, then a release of it, and installs that. */
export function installFromGitHub(): void {
  openQuickPick({
    placeholder: 'Install an extension from a GitHub repository (owner/name, or its address)',
    items: [],
    input: {
      value: '',
      accept: text => {
        const repository = text.trim()
        if (repository) {
          void chooseRelease(repository)
        }
      },
    },
  })
}

export async function uninstall(extension: ExtensionStatus): Promise<void> {
  if (installBusy.get()) {
    return
  }
  installBusy.set(true)
  try {
    if (await uninstallExtension(extension.id)) {
      notify('info', `Uninstalled ${extension.name}.`)
      await refreshSources()
    }
  } catch (error) {
    reportError(`Uninstalling ${extension.name}`, error)
  } finally {
    installBusy.set(false)
  }
}

/** Installs the update the last check found for an extension. */
export function update(source: ExtensionSource): Promise<void> {
  return installRelease(source.repository, source.update)
}

/**
 * Looks for newer releases of the extensions installed from GitHub. When
 * `quiet`, says nothing unless there are some.
 */
export async function checkForUpdates(quiet = false): Promise<void> {
  let updates
  try {
    updates = await checkExtensionUpdates()
  } catch (error) {
    if (!quiet) {
      reportError('Checking for extension updates', error)
    }
    return
  }
  await refreshSources()
  if (updates.length === 0) {
    if (!quiet) {
      notify('info', 'All extensions installed from GitHub are up to date.')
    }
    return
  }
  const what =
    updates.length === 1
      ? `An update to ${updates[0]?.tag ?? 'a newer release'} is available for ${updates[0]?.id ?? 'an extension'}.`
      : `${updates.length} extension updates are available.`
  notify('info', what, [
    {
      label: updates.length === 1 ? 'Update' : 'Update All',
      run: async () => {
        for (const found of updates) {
          await installRelease(found.repository, found.tag)
        }
      },
    },
    { label: 'Show Extensions', run: () => showView('extensions', true) },
  ])
}
