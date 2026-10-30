import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('../ipc/terminal', () => ({
  createTerminal: vi.fn(),
  listTerminals: vi.fn(),
  attachTerminal: vi.fn(),
  writeTerminal: vi.fn(),
  resizeTerminal: vi.fn(),
  acknowledgeTerminal: vi.fn(),
  killTerminal: vi.fn(),
  terminalPlatform: vi.fn(async () => ({ windowsBuild: null })),
  terminalProfiles: vi.fn(),
  setDefaultProfile: vi.fn(),
}))

const ipc = await import('../ipc/terminal')
const { openDefaultProfileMenu, openNewTerminalMenu } = await import('./profileMenu')
const { contextMenuStore } = await import('../components/contextMenuState')
const { notifications } = await import('../state/notifications')

const profiles = [
  { name: 'Windows PowerShell', default: true },
  { name: 'Git Bash', default: false },
  { name: 'Ubuntu (WSL)', default: false },
]

const menu = () => contextMenuStore.get()?.entries ?? []
const labels = () => menu().map(entry => entry?.label ?? '—')
const choose = (label: string) =>
  menu()
    .find(entry => entry?.label === label)!
    .run()

describe('terminal profile menus', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    contextMenuStore.set(null)
    notifications.set([])
    vi.mocked(ipc.terminalProfiles).mockResolvedValue(profiles)
    vi.mocked(ipc.createTerminal).mockResolvedValue({ id: 1, title: 'Ubuntu (WSL)' })
    vi.mocked(ipc.setDefaultProfile).mockResolvedValue(undefined)
  })

  it('starts a terminal with the chosen profile', async () => {
    await openNewTerminalMenu(10, 20)
    expect(labels()).toEqual([
      'Windows PowerShell (default)',
      'Git Bash',
      'Ubuntu (WSL)',
      '—',
      'Select Default Profile…',
      'Configure Profiles…',
    ])
    await choose('Ubuntu (WSL)')
    expect(ipc.createTerminal).toHaveBeenCalledWith(80, 24, 'Ubuntu (WSL)', expect.any(Function))
  })

  it('makes the chosen profile the default', async () => {
    await openDefaultProfileMenu(10, 20)
    expect(labels()).toEqual(['Windows PowerShell (default)', 'Git Bash', 'Ubuntu (WSL)'])
    await choose('Git Bash')
    expect(ipc.setDefaultProfile).toHaveBeenCalledWith('Git Bash')
  })

  it('says when no shell was found, and reports failures', async () => {
    vi.mocked(ipc.terminalProfiles).mockResolvedValueOnce([])
    await openDefaultProfileMenu(10, 20)
    expect(menu()).toEqual([
      expect.objectContaining({ label: 'No shells were found', disabled: true }),
    ])

    contextMenuStore.set(null)
    vi.mocked(ipc.terminalProfiles).mockRejectedValueOnce(new Error('wsl.exe hung'))
    await openNewTerminalMenu(10, 20)
    expect(contextMenuStore.get()).toBeNull()
    expect(notifications.get()).toEqual([
      expect.objectContaining({ kind: 'error', message: expect.stringContaining('wsl.exe hung') }),
    ])
  })
})
