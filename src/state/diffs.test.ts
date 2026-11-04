import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { DocumentSummary } from '../ipc/editor'

vi.mock('../ipc/editor', () => ({
  openDocument: vi.fn(),
  openProvidedDocument: vi.fn(),
  closeDocument: vi.fn(),
  hideDocument: vi.fn(),
  closeView: vi.fn(),
}))

const ipc = await import('../ipc/editor')
const documents = await import('./documents')
const diffs = await import('./diffs')

const summary = (id: number, overrides: Partial<DocumentSummary> = {}): DocumentSummary => ({
  id,
  title: `file${id}.rs`,
  path: null,
  dirty: false,
  lineEnding: 'LF',
  encoding: 'UTF-8',
  encodingId: 'utf-8',
  ...overrides,
})

const spec = { original: 'git::src/a.rs', modified: '/repo/src/a.rs', title: 'a.rs (Working Tree)' }

describe('diff tabs', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(ipc.closeView).mockResolvedValue()
    documents.documentsStore.set({ documents: [], activeId: null })
    diffs.diffsStore.set({})
  })

  it('name what they compare', () => {
    const item = diffs.diffItem(spec)
    expect(item.startsWith('diff:')).toBe(true)
    expect(diffs.parseDiffItem(item)).toEqual(spec)
    expect(diffs.parseDiffItem('diff:nonsense')).toBeNull()
    expect(diffs.parseDiffItem('document:1')).toBeNull()
    expect(diffs.isAddress('git:HEAD:a.rs')).toBe(true)
    expect(diffs.isAddress('C:\\repo\\a.rs')).toBe(false)
    expect(diffs.isAddress('/repo/a.rs')).toBe(false)
  })

  it('open their sides hidden and close them with the last tab', async () => {
    vi.mocked(ipc.openProvidedDocument).mockResolvedValue(summary(1))
    vi.mocked(ipc.openDocument).mockResolvedValue(summary(2, { path: '/repo/src/a.rs' }))
    vi.mocked(ipc.closeDocument).mockResolvedValue(true)
    const item = diffs.diffItem(spec)
    await diffs.loadDiff(item)
    expect(ipc.openProvidedDocument).toHaveBeenCalledWith('git::src/a.rs', true)
    expect(ipc.openDocument).toHaveBeenCalledWith('/repo/src/a.rs', false, true)
    expect(diffs.diffsStore.get()[item]).toMatchObject({ status: 'ready', dirty: false })
    expect(diffs.diffShows(2)).toBe(true)

    // Another tab shows the same file: it stays open for it.
    const other = diffs.diffItem({ ...spec, original: 'git:HEAD:src/a.rs', title: 'a.rs (Index)' })
    vi.mocked(ipc.openProvidedDocument).mockResolvedValueOnce(summary(3))
    await diffs.loadDiff(other)
    expect(await diffs.releaseDiff(item)).toBe(true)
    expect(ipc.closeDocument).toHaveBeenCalledTimes(1)
    expect(ipc.closeDocument).toHaveBeenCalledWith(1)
    expect(await diffs.releaseDiff(other)).toBe(true)
    expect(
      vi
        .mocked(ipc.closeDocument)
        .mock.calls.map(([id]) => id)
        .sort(),
    ).toEqual([1, 2, 3])
    expect(diffs.diffsStore.get()).toEqual({})
  })

  it('stay open when saving is cancelled', async () => {
    vi.mocked(ipc.openProvidedDocument).mockResolvedValue(summary(1))
    vi.mocked(ipc.openDocument).mockResolvedValue(summary(2))
    vi.mocked(ipc.closeDocument).mockResolvedValue(false)
    const item = diffs.diffItem(spec)
    await diffs.loadDiff(item)
    diffs.setDiffDirty(item, true)
    expect(await diffs.releaseDiff(item)).toBe(false)
    expect(diffs.diffsStore.get()[item]).toMatchObject({ status: 'ready', dirty: true })
  })

  it('leave documents with tabs of their own open, and keep them open when those close', async () => {
    const file = summary(2, { path: '/repo/src/a.rs' })
    documents.documentsStore.set({ documents: [file], activeId: 2 })
    vi.mocked(ipc.openProvidedDocument).mockResolvedValue(summary(1))
    vi.mocked(ipc.openDocument).mockResolvedValue(file)
    vi.mocked(ipc.closeDocument).mockResolvedValue(true)
    const item = diffs.diffItem(spec)
    await diffs.loadDiff(item)

    // Its own tab closes: it's only hidden, as the diff shows it.
    vi.mocked(ipc.hideDocument).mockResolvedValue()
    await documents.close(2)
    expect(ipc.hideDocument).toHaveBeenCalledWith(2)
    expect(ipc.closeDocument).not.toHaveBeenCalled()
    expect(documents.documentsStore.get().documents).toEqual([])

    expect(await diffs.releaseDiff(item)).toBe(true)
    expect(
      vi
        .mocked(ipc.closeDocument)
        .mock.calls.map(([id]) => id)
        .sort(),
    ).toEqual([1, 2])
  })

  it('report sides that could not open', async () => {
    vi.mocked(ipc.openProvidedDocument).mockRejectedValue(new Error('No extension provides git:'))
    vi.mocked(ipc.openDocument).mockResolvedValue(summary(2))
    vi.mocked(ipc.closeDocument).mockResolvedValue(true)
    const item = diffs.diffItem(spec)
    await diffs.loadDiff(item)
    expect(diffs.diffsStore.get()[item]).toEqual({
      status: 'failed',
      message: 'No extension provides git:',
    })
    // The side that opened doesn't stay open hidden.
    expect(ipc.closeDocument).toHaveBeenCalledWith(2)
  })
})
