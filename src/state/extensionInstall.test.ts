import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('../ipc/extensionInstall', () => ({
  checkExtensionUpdates: vi.fn(() => Promise.resolve([])),
  getExtensionCatalog: vi.fn(() => Promise.resolve([])),
  installExtension: vi.fn(() => Promise.resolve(null)),
  listExtensionReleases: vi.fn(),
  listExtensionSources: vi.fn(() => Promise.resolve([])),
  uninstallExtension: vi.fn(() => Promise.resolve(false)),
}))

const ipc = await import('../ipc/extensionInstall')
const { paletteStore } = await import('../components/paletteState')
const { notifications } = await import('./notifications')
const flows = await import('./extensionInstall')

const release = (
  tag: string,
  extra: Partial<Awaited<ReturnType<typeof ipc.listExtensionReleases>>['releases'][number]> = {},
) => ({
  tag,
  name: null,
  prerelease: false,
  publishedAt: '2026-11-05T10:00:00Z',
  url: `https://github.com/FlareiteLabs/hello/releases/${tag}`,
  installable: true,
  ...extra,
})

describe('installing extensions from GitHub', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    notifications.set([])
    paletteStore.set({ open: false, initial: '', session: 0, pick: null })
  })

  it('asks for a repository, then lists its installable releases to choose from', async () => {
    vi.mocked(ipc.listExtensionReleases).mockResolvedValue({
      repository: 'FlareiteLabs/hello',
      releases: [
        release('v2.0.0-beta.1', { prerelease: true }),
        release('v1.0.0', { name: 'First' }),
        release('v0.1.0', { installable: false }),
      ],
    })
    flows.installFromGitHub()
    const input = paletteStore.get().pick?.input
    expect(input).toBeDefined()
    input?.accept('  https://github.com/FlareiteLabs/hello  ')
    expect(ipc.listExtensionReleases).toHaveBeenCalledWith('https://github.com/FlareiteLabs/hello')
    await vi.waitFor(() => expect(paletteStore.get().pick?.items).toHaveLength(2))

    const pick = paletteStore.get().pick
    expect(pick?.placeholder).toBe('Choose a release of FlareiteLabs/hello to install')
    expect(pick?.items.map(item => [item.label, item.detail])).toEqual([
      ['v2.0.0-beta.1', 'Pre-release · 2026-11-05'],
      ['v1.0.0 — First', 'Latest · 2026-11-05'],
    ])
    pick?.items[1]?.run()
    expect(ipc.installExtension).toHaveBeenCalledWith('FlareiteLabs/hello', 'v1.0.0')
  })

  it('says when a repository has nothing to install', async () => {
    vi.mocked(ipc.listExtensionReleases).mockResolvedValue({
      repository: 'FlareiteLabs/hello',
      releases: [release('v0.1.0', { installable: false })],
    })
    await flows.chooseRelease('FlareiteLabs/hello')
    expect(paletteStore.get().pick).toBeNull()
    expect(notifications.get()[0]?.message).toBe(
      'FlareiteLabs/hello has no releases with an extension.zip to install.',
    )
  })

  it('says what was installed', async () => {
    vi.mocked(ipc.installExtension).mockResolvedValueOnce({
      id: 'test.hello',
      name: 'Hello',
      version: '1.0.0',
    })
    await flows.installRelease('FlareiteLabs/hello', 'v1.0.0')
    expect(notifications.get()[0]?.message).toBe('Installed Hello 1.0.0.')
    expect(ipc.listExtensionSources).toHaveBeenCalled()
    expect(flows.installBusy.get()).toBe(false)
  })

  it('offers the updates a check finds, and is quiet without any when asked to be', async () => {
    await flows.checkForUpdates(true)
    expect(notifications.get()).toEqual([])
    await flows.checkForUpdates()
    expect(notifications.get()[0]?.message).toBe(
      'All extensions installed from GitHub are up to date.',
    )

    notifications.set([])
    vi.mocked(ipc.checkExtensionUpdates).mockResolvedValueOnce([
      { id: 'test.hello', repository: 'FlareiteLabs/hello', version: '1.0.0', tag: 'v1.1.0' },
    ])
    await flows.checkForUpdates(true)
    const [found] = notifications.get()
    expect(found?.message).toBe('An update to v1.1.0 is available for test.hello.')
    expect(found?.actions?.map(action => action.label)).toEqual(['Update', 'Show Extensions'])
    await found?.actions?.[0]?.run()
    expect(ipc.installExtension).toHaveBeenCalledWith('FlareiteLabs/hello', 'v1.1.0')
  })
})
