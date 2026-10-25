import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ExtensionStatus } from '../ipc/extensions'

vi.mock('../ipc/extensions', () => ({
  listExtensions: vi.fn(),
  runExtensionCommand: vi.fn(),
  setExtensionPermissions: vi.fn(),
  restartExtension: vi.fn(),
}))

const ipc = await import('../ipc/extensions')
const { getCommand, executeCommand } = await import('../commands/registry')
const { activeEditor } = await import('../editor/activeEditor')
const { notifications } = await import('./notifications')
const { applyStatuses, extensionsStore, showExtensionFailure } = await import('./extensions')

function extension(overrides: Partial<ExtensionStatus> = {}): ExtensionStatus {
  return {
    id: 'example.tidy',
    name: 'Tidy',
    version: '0.1.0',
    description: '',
    path: '/ext/tidy',
    development: false,
    state: 'active',
    error: null,
    permissions: [
      {
        permission: 'documents-read',
        description: 'Read open documents',
        granted: true,
        decided: true,
      },
    ],
    commands: [{ id: 'example.tidy.trim', title: 'Trim', category: 'Tidy', hidden: false }],
    configErrors: [],
    contributions: { themes: [], keybindings: [], menus: [], views: [], decorations: [] },
    usage: {
      busyMs: 0,
      calls: 0,
      slowestCallMs: 0,
      memoryBytes: 0,
      peakMemoryBytes: 0,
      droppedEvents: 0,
      compileMs: 0,
    },
    ...overrides,
  }
}

describe('extensions state', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    notifications.set([])
    applyStatuses([])
  })

  it('registers the commands of usable extensions in the palette', async () => {
    applyStatuses([extension()])
    expect(extensionsStore.get()).toHaveLength(1)
    expect(getCommand('example.tidy.trim')).toMatchObject({ title: 'Trim', category: 'Tidy' })

    activeEditor.set({ documentId: 4 } as never)
    await executeCommand('example.tidy.trim')
    expect(ipc.runExtensionCommand).toHaveBeenCalledWith('example.tidy.trim', 4, null)
    activeEditor.set(null)

    applyStatuses([extension({ state: 'disabled' })])
    expect(getCommand('example.tidy.trim')).toBeUndefined()
  })

  it('asks once for permissions an extension is waiting for', async () => {
    const waiting = extension({
      id: 'example.waiting',
      state: 'awaitingPermission',
      permissions: [
        {
          permission: 'documents-write',
          description: 'Edit open documents',
          granted: false,
          decided: false,
        },
      ],
    })
    applyStatuses([waiting])
    applyStatuses([waiting])
    const prompts = notifications.get()
    expect(prompts).toHaveLength(1)
    expect(prompts[0]?.message).toContain('Tidy wants to: edit open documents')
    const allow = prompts[0]?.actions?.find(action => action.label === 'Allow')
    await allow?.run()
    expect(ipc.setExtensionPermissions).toHaveBeenCalledWith('example.waiting', ['documents-write'])
  })

  it('offers to restart an extension that stopped', async () => {
    showExtensionFailure('example.tidy', 'Tidy', 'crashed')
    const [failure] = notifications.get()
    expect(failure).toMatchObject({ kind: 'error' })
    expect(failure?.message).toBe('The extension Tidy stopped: crashed')
    await failure?.actions?.find(action => action.label === 'Restart')?.run()
    expect(ipc.restartExtension).toHaveBeenCalledWith('example.tidy')
  })
})
