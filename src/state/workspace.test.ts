import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('../ipc/workspace', () => ({ listDirectory: vi.fn() }))

const ipc = await import('../ipc/workspace')
const { DEFAULT_LAYOUT, layoutStore } = await import('./layout')
const { foldersBetween, revealPath, workspaceStore } = await import('./workspace')

describe('workspace paths', () => {
  it('lists the folders between the root and a path', () => {
    expect(foldersBetween('/w', '/w/src/net/http.rs')).toEqual(['/w/src', '/w/src/net'])
    expect(foldersBetween('/w/', '/w/README.md')).toEqual([])
    expect(foldersBetween('D:\\code\\w', 'D:\\code\\w\\src\\main.rs')).toEqual(['D:\\code\\w\\src'])
  })

  it('refuses paths outside the root, and the root itself', () => {
    expect(foldersBetween('/w', '/work/a.txt')).toBeNull()
    expect(foldersBetween('/w', '/elsewhere/a.txt')).toBeNull()
    expect(foldersBetween('/w', '/w')).toBeNull()
    expect(foldersBetween('/w', '/w/')).toBeNull()
  })
})

describe('revealPath', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    layoutStore.set({ ...DEFAULT_LAYOUT, expanded: ['/w/docs'] })
    vi.mocked(ipc.listDirectory).mockResolvedValue([])
    workspaceStore.set({
      info: { root: '/w', name: 'w', trust: 'trusted' },
      listings: new Map([['/w/src', []]]),
      indexing: false,
    })
  })

  it('expands and loads the folders down to a path', async () => {
    expect(await revealPath('/w/src/net/http.rs')).toBe(true)
    expect(layoutStore.get().expanded).toEqual(['/w/docs', '/w/src', '/w/src/net'])
    // `/w/src` was already loaded.
    expect(ipc.listDirectory).toHaveBeenCalledTimes(1)
    expect(ipc.listDirectory).toHaveBeenCalledWith('/w/src/net')
  })

  it('does nothing outside the workspace', async () => {
    expect(await revealPath('/elsewhere/a.txt')).toBe(false)
    expect(layoutStore.get().expanded).toEqual(['/w/docs'])
    expect(ipc.listDirectory).not.toHaveBeenCalled()
  })
})
