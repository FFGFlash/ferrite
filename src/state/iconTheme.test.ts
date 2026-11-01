import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ExtensionStatus, IconAssociations } from '../ipc/extensions'

vi.mock('../ipc/extensions', () => ({ getIconTheme: vi.fn() }))

const ipc = await import('../ipc/extensions')
const { extensionsStore } = await import('./extensions')
const { iconThemeStore, setActiveIconTheme } = await import('./iconTheme')
const { notifications } = await import('./notifications')

const none: IconAssociations = {
  file: null,
  folder: null,
  folderExpanded: null,
  fileExtensions: {},
  fileNames: {},
  folderNames: {},
  folderNamesExpanded: {},
}

const icons = (state: ExtensionStatus['state'] = 'ready') =>
  ({
    id: 'example.icons',
    name: 'Icons',
    version: '1.0.0',
    description: '',
    path: '',
    development: false,
    state,
    error: null,
    permissions: [],
    commands: [],
    configErrors: [],
    contributions: {
      themes: [],
      iconThemes: [{ id: 'example.icons.colorful', label: 'Colorful' }],
      keybindings: [],
      menus: [],
      views: [],
      decorations: [],
      languages: [],
      languageServers: [],
    },
  }) as unknown as ExtensionStatus

const settle = () => new Promise(resolve => setTimeout(resolve))

describe('icon themes', () => {
  beforeEach(async () => {
    vi.clearAllMocks()
    notifications.set([])
    extensionsStore.set([])
    setActiveIconTheme('default')
    await settle()
    vi.mocked(ipc.getIconTheme).mockResolvedValue({
      icons: { rust: 'data:image/svg+xml;base64,AA==', constructor: 'data:x' },
      associations: { ...none, fileExtensions: { rs: 'rust' } },
      light: none,
    })
  })

  it('loads a theme once its extension is there, as maps', async () => {
    setActiveIconTheme('example.icons.colorful')
    await settle()
    expect(ipc.getIconTheme).not.toHaveBeenCalled()
    expect(iconThemeStore.get()).toBeNull()

    extensionsStore.set([icons()])
    await settle()
    expect(ipc.getIconTheme).toHaveBeenCalledWith('example.icons.colorful')
    const theme = iconThemeStore.get()
    expect(theme?.id).toBe('example.icons.colorful')
    expect(theme?.associations.fileExtensions.get('rs')).toBe('rust')
    expect(theme?.associations.fileNames.get('constructor')).toBeUndefined()
  })

  it('goes back to the built-in icons when the extension is turned off', async () => {
    extensionsStore.set([icons()])
    setActiveIconTheme('example.icons.colorful')
    await settle()
    expect(iconThemeStore.get()).not.toBeNull()

    extensionsStore.set([icons('disabled')])
    await settle()
    expect(iconThemeStore.get()).toBeNull()

    extensionsStore.set([icons()])
    await settle()
    setActiveIconTheme('default')
    await settle()
    expect(iconThemeStore.get()).toBeNull()
  })

  it('reports a theme that fails to load', async () => {
    vi.mocked(ipc.getIconTheme).mockRejectedValueOnce(new Error('broken'))
    extensionsStore.set([icons()])
    setActiveIconTheme('example.icons.colorful')
    await settle()
    expect(iconThemeStore.get()).toBeNull()
    expect(notifications.get()[0]?.message).toContain('file icon theme')
  })
})
