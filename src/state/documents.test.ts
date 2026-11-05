import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { DocumentSummary } from '../ipc/editor'
import type { FileTab } from './viewers'

vi.mock('../ipc/editor', () => ({
  listDocuments: vi.fn(),
  newDocument: vi.fn(),
  pickFile: vi.fn(),
  saveDocument: vi.fn(),
  saveDocumentAs: vi.fn(),
  closeDocument: vi.fn(),
  openDocument: vi.fn(),
}))

const ipc = await import('../ipc/editor')
const documents = await import('./documents')
const { notifications } = await import('./notifications')
const { extensionsStore } = await import('./extensions')
const viewers = await import('./viewers')
const { CommandError } = await import('../ipc')

const summary = (id: number, overrides: Partial<DocumentSummary> = {}): DocumentSummary => ({
  id,
  title: `file${id}.txt`,
  path: null,
  dirty: false,
  lineEnding: 'LF',
  encoding: 'UTF-8',
  encodingId: 'utf-8',
  ...overrides,
})

describe('documents store', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    documents.documentsStore.set({ documents: [], activeId: null })
    notifications.set([])
  })

  it('adds new and opened documents and activates them', async () => {
    vi.mocked(ipc.newDocument).mockResolvedValue(summary(1))
    vi.mocked(ipc.pickFile).mockResolvedValueOnce('/w/file2.txt').mockResolvedValueOnce(null)
    vi.mocked(ipc.openDocument).mockResolvedValueOnce(summary(2))
    await documents.newFile()
    await documents.openFile()
    await documents.openFile()
    expect(documents.documentsStore.get()).toEqual({
      documents: [summary(1), summary(2)],
      activeId: 2,
    })
  })

  it('activates a neighbour when the active document closes', async () => {
    documents.documentsStore.set({ documents: [summary(1), summary(2), summary(3)], activeId: 2 })
    vi.mocked(ipc.closeDocument).mockResolvedValue(true)
    await documents.close(2)
    expect(documents.documentsStore.get()).toEqual({
      documents: [summary(1), summary(3)],
      activeId: 3,
    })
    await documents.close(3)
    expect(documents.documentsStore.get().activeId).toBe(1)
  })

  it('keeps the document when closing is cancelled', async () => {
    documents.documentsStore.set({ documents: [summary(1)], activeId: 1 })
    vi.mocked(ipc.closeDocument).mockResolvedValue(false)
    await documents.close()
    expect(documents.documentsStore.get().documents).toHaveLength(1)
  })

  it('flushes pending edits before saving', async () => {
    documents.documentsStore.set({ documents: [summary(1, { dirty: true })], activeId: 1 })
    const order: string[] = []
    const unregister = documents.registerBeforeSave(1, async () => {
      order.push('flush')
    })
    vi.mocked(ipc.saveDocument).mockImplementation(async () => {
      order.push('save')
      return summary(1, { title: 'saved.txt' })
    })
    await documents.save()
    unregister()
    expect(order).toEqual(['flush', 'save'])
    expect(documents.documentsStore.get().documents[0]?.title).toBe('saved.txt')
  })

  it('reports failures as notifications', async () => {
    documents.documentsStore.set({ documents: [summary(1)], activeId: 1 })
    vi.mocked(ipc.saveDocument).mockRejectedValue(new Error('disk full'))
    await documents.save()
    expect(notifications.get()).toEqual([
      expect.objectContaining({ kind: 'error', message: expect.stringContaining('disk full') }),
    ])
  })

  it('opens a path at a location once its editor mounts', async () => {
    vi.mocked(ipc.openDocument).mockResolvedValue(summary(4, { path: '/w/a.rs' }))
    await documents.openPath('/w/a.rs', { start: { line: 2, column: 1 } })
    expect(documents.documentsStore.get().activeId).toBe(4)
    const pending = documents.pendingSelection(4)
    expect(pending).toEqual({ start: { line: 2, column: 1 }, end: { line: 2, column: 1 } })
    // Stays pending until an editor has applied it.
    expect(documents.pendingSelection(4)).toBe(pending)
    documents.clearPendingSelection(4, pending!)
    expect(documents.pendingSelection(4)).toBeUndefined()
  })

  it('updates dirty state only when it changes', () => {
    documents.documentsStore.set({ documents: [summary(1)], activeId: 1 })
    const before = documents.documentsStore.get()
    documents.setDirty(1, false)
    expect(documents.documentsStore.get()).toBe(before)
    documents.setDirty(1, true)
    expect(documents.documentsStore.get().documents[0]?.dirty).toBe(true)
  })

  describe('files that open other than as text', () => {
    const shown: (FileTab | null)[] = []
    beforeEach(() => {
      shown.length = 0
      viewers.setFileTabHandler((_, tab) => {
        shown.push(tab)
        return false
      })
      extensionsStore.set([])
    })

    const imagesExtension = (priority: 'default' | 'option') =>
      ({
        id: 'flareite.images',
        state: 'inactive',
        contributions: {
          viewers: [
            {
              id: 'image',
              fullId: 'flareite.images.image',
              title: 'Image Viewer',
              patterns: ['*.png'],
              command: 'flareite.images.show',
              priority,
            },
          ],
        },
      }) as never

    it('opens files in the viewer that is their default', async () => {
      extensionsStore.set([imagesExtension('default')])
      expect(await documents.openPath('/w/cat.PNG')).toBeNull()
      expect(ipc.openDocument).not.toHaveBeenCalled()
      expect(shown).toEqual([null, { viewer: 'flareite.images.image', path: '/w/cat.PNG' }])
    })

    it('asks before opening a binary file as text, unless told to', async () => {
      extensionsStore.set([imagesExtension('option')])
      vi.mocked(ipc.openDocument).mockRejectedValueOnce(new CommandError('binary', 'binary'))
      expect(await documents.openPath('/w/cat.png')).toBeNull()
      expect(shown.at(-1)).toEqual({ viewer: null, path: '/w/cat.png' })
      expect(notifications.get()).toEqual([])

      vi.mocked(ipc.openDocument).mockResolvedValueOnce(summary(5, { path: '/w/cat.png' }))
      const opened = await documents.openPath('/w/cat.png', undefined, { asText: true })
      expect(opened?.id).toBe(5)
      expect(ipc.openDocument).toHaveBeenLastCalledWith('/w/cat.png', true)
    })
  })
})
