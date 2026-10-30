import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { DocumentSummary } from '../ipc/editor'

vi.mock('../ipc/editor', () => ({
  listDocuments: vi.fn(),
  newDocument: vi.fn(),
  openDocumentDialog: vi.fn(),
  saveDocument: vi.fn(),
  saveDocumentAs: vi.fn(),
  closeDocument: vi.fn(),
  openDocument: vi.fn(),
}))

const ipc = await import('../ipc/editor')
const documents = await import('./documents')
const { notifications } = await import('./notifications')

const summary = (id: number, overrides: Partial<DocumentSummary> = {}): DocumentSummary => ({
  id,
  title: `file${id}.txt`,
  path: null,
  dirty: false,
  lineEnding: 'LF',
  encoding: 'UTF-8',
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
    vi.mocked(ipc.openDocumentDialog).mockResolvedValueOnce(summary(2)).mockResolvedValueOnce(null)
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
})
