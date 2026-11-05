import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ExtensionStatus, PreviewContent } from '../ipc/extensions'

vi.mock('../ipc/extensions', () => ({
  openViewer: vi.fn(),
  getPreviewContent: vi.fn(),
  getPreviewImage: vi.fn(async () => 'data:image/png;base64,AAAA'),
  listPreviews: vi.fn(async () => []),
  previewClosed: vi.fn(async () => {}),
}))
vi.mock('../dock/dock', () => ({ reopenWith: vi.fn(async () => {}) }))

const { axeViolations } = await import('../test/axe')
const ipc = await import('../ipc/extensions')
const { reopenWith } = await import('../dock/dock')
const { extensionsStore } = await import('../state/extensions')
const { previewChanged } = await import('../state/previews')
const viewers = await import('../state/viewers')
const { ViewerPane } = await import('./ViewerPane')

const VIEWER = 'flareite.images.image'
const PATH = '/w/cat.png'
const ID = `${VIEWER}/${PATH}`

const images = (state: ExtensionStatus['state'] = 'active') =>
  ({
    id: 'flareite.images',
    name: 'Images',
    state,
    contributions: {
      viewers: [
        {
          id: 'image',
          fullId: VIEWER,
          title: 'Image Viewer',
          patterns: ['*.png'],
          command: 'flareite.images.show',
          priority: 'default',
        },
      ],
    },
  }) as unknown as ExtensionStatus

const picture: PreviewContent = {
  nodes: [
    { type: 'paragraph', parent: null },
    {
      type: 'image',
      parent: 0,
      source: { kind: 'file', path: PATH },
      description: 'cat.png',
      title: null,
    },
    { type: 'paragraph', parent: null },
    { type: 'text', parent: 2, text: 'PNG · 3 × 2 pixels · 70 bytes' },
  ],
  highlights: {},
  highlightsPending: false,
}

describe('ViewerPane', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    extensionsStore.set([])
    vi.mocked(ipc.openViewer).mockResolvedValue(ID)
    vi.mocked(ipc.getPreviewContent).mockResolvedValue(picture)
  })

  it('shows what the extension fills it in with, and asks again when the file changes', async () => {
    extensionsStore.set([images()])
    const { container } = render(
      <ViewerPane item={`viewer:${VIEWER}:${PATH}`} viewer={VIEWER} path={PATH} />,
    )
    await waitFor(() =>
      expect(screen.getByRole('img', { name: 'cat.png' })).toHaveAttribute(
        'src',
        'data:image/png;base64,AAAA',
      ),
    )
    expect(screen.getByText('PNG · 3 × 2 pixels · 70 bytes')).toBeInTheDocument()
    expect(ipc.openViewer).toHaveBeenCalledWith(VIEWER, PATH)
    expect(await axeViolations(container)).toEqual([])

    act(() => viewers.filesChanged([PATH, '/w/other.png']))
    await waitFor(() => expect(ipc.openViewer).toHaveBeenCalledTimes(2))
    // The picture is read again too.
    await waitFor(() => expect(ipc.getPreviewImage).toHaveBeenCalledTimes(2))
    act(() => previewChanged(ID))
    await waitFor(() => expect(ipc.getPreviewContent).toHaveBeenCalledTimes(2))
  })

  it('says why when the extension fails or is turned off, and opens the file as text', async () => {
    const user = userEvent.setup()
    extensionsStore.set([images()])
    vi.mocked(ipc.openViewer).mockRejectedValueOnce(new Error("the file can't be read"))
    const item = `viewer:${VIEWER}:${PATH}`
    const { container } = render(<ViewerPane item={item} viewer={VIEWER} path={PATH} />)
    expect(
      await screen.findByText("Image Viewer couldn't show cat.png: the file can't be read"),
    ).toBeInTheDocument()
    expect(await axeViolations(container)).toEqual([])
    await user.click(screen.getByRole('button', { name: 'Try Again' }))
    await waitFor(() => expect(screen.getByRole('img', { name: 'cat.png' })).toHaveAttribute('src'))

    act(() => extensionsStore.set([images('disabled')]))
    expect(
      screen.getByText('cat.png opens in Image Viewer, but its extension is turned off.'),
    ).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Open as Text' }))
    expect(reopenWith).toHaveBeenCalledWith(item, null)
  })

  it('asks before opening a binary file as text, and offers viewers that can show it', async () => {
    const user = userEvent.setup()
    const shown: unknown[] = []
    viewers.setFileTabHandler((_, tab) => {
      shown.push(tab)
      return true
    })
    const item = `viewer::/w/data.bin`
    const { container } = render(<ViewerPane item={item} viewer={null} path="/w/data.bin" />)
    expect(
      screen.getByText(
        "data.bin isn't shown because it's either binary or uses an unsupported text encoding.",
      ),
    ).toBeInTheDocument()
    expect(await axeViolations(container)).toEqual([])
    await user.click(screen.getByRole('button', { name: 'Open Anyway' }))
    expect(reopenWith).toHaveBeenCalledWith(item, null)

    // A viewer that's the default for it takes over once its extension loads.
    act(() => {
      render(<ViewerPane item="viewer::/w/cat.png" viewer={null} path={PATH} />)
      extensionsStore.set([images('inactive')])
    })
    await waitFor(() => expect(shown).toContainEqual({ viewer: VIEWER, path: PATH }))
  })
})
