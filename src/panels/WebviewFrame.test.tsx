import { act, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('../ipc/extensions', () => ({
  getWebview: vi.fn(),
  sendWebviewMessage: vi.fn(async () => {}),
  setWebviewState: vi.fn(async () => {}),
  webviewStorage: vi.fn(async () => null),
}))

const ipc = await import('../ipc/extensions')
const { registerCommands } = await import('../commands/registry')
const webviews = await import('../state/webviews')
const { WebviewFrame } = await import('./WebviewFrame')

const ID = 'ferrite.images.svg//w/cat.svg'
const URL = 'http://ferrite-webview.localhost/abc/media/viewer.html'

/** A message from `source`, as the page's bridge sends them. */
const fromPage = (source: Window | null, data: unknown) =>
  act(() => {
    window.dispatchEvent(new MessageEvent('message', { data, source }))
  })

describe('WebviewFrame', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    webviews.webviewClosed(ID)
    vi.mocked(ipc.getWebview).mockResolvedValue({ url: URL, version: 0 })
  })

  it('shows the page sandboxed, and passes messages between it and the extension', async () => {
    // Sent before the page loaded: it waits.
    webviews.webviewMessage(ID, '{"size":"64 × 64"}')
    render(<WebviewFrame id={ID} title="cat.svg" fileVersion={0} />)
    const frame = await screen.findByTitle<HTMLIFrameElement>('cat.svg')
    expect(frame).toHaveAttribute('sandbox', 'allow-scripts')
    expect(frame).toHaveAttribute('src', URL)
    const page = frame.contentWindow!
    const posted = vi.spyOn(page, 'postMessage')

    fromPage(page, { ferrite: 'webview', type: 'ready' })
    expect(posted).toHaveBeenCalledWith(
      { ferrite: 'webview', type: 'message', data: '{"size":"64 × 64"}' },
      '*',
    )
    act(() => webviews.webviewMessage(ID, '2'))
    expect(posted).toHaveBeenLastCalledWith({ ferrite: 'webview', type: 'message', data: '2' }, '*')

    fromPage(page, { ferrite: 'webview', type: 'message', data: '{"zoom":2}' })
    expect(ipc.sendWebviewMessage).toHaveBeenCalledWith(ID, '{"zoom":2}')

    // Other windows, and other shapes, are ignored.
    fromPage(window, { ferrite: 'webview', type: 'message', data: '"spoofed"' })
    fromPage(page, { type: 'message', data: '"unmarked"' })
    fromPage(page, { ferrite: 'webview', type: 'message', data: { not: 'text' } })
    expect(ipc.sendWebviewMessage).toHaveBeenCalledTimes(1)
  })

  it('tells the page when its file changes, and loads a page the extension replaced', async () => {
    const { rerender } = render(<WebviewFrame id={ID} title="cat.svg" fileVersion={0} />)
    const frame = await screen.findByTitle<HTMLIFrameElement>('cat.svg')
    const posted = vi.spyOn(frame.contentWindow!, 'postMessage')
    fromPage(frame.contentWindow, { ferrite: 'webview', type: 'ready' })
    rerender(<WebviewFrame id={ID} title="cat.svg" fileVersion={1} />)
    expect(posted).toHaveBeenCalledWith(
      { ferrite: 'webview', type: 'fileChanged', data: undefined },
      '*',
    )

    vi.mocked(ipc.getWebview).mockResolvedValue({
      url: 'http://ferrite-webview.localhost/abc/',
      version: 1,
    })
    act(() => webviews.webviewChanged(ID))
    await waitFor(() =>
      expect(screen.getByTitle('cat.svg')).toHaveAttribute(
        'src',
        'http://ferrite-webview.localhost/abc/?v=1',
      ),
    )
  })

  it('saves the page’s state and answers its storage requests', async () => {
    render(<WebviewFrame id={ID} title="cat.svg" />)
    const frame = await screen.findByTitle<HTMLIFrameElement>('cat.svg')
    const page = frame.contentWindow!
    const posted = vi.spyOn(page, 'postMessage')

    fromPage(page, { ferrite: 'webview', type: 'setState', data: '{"zoom":2}' })
    expect(ipc.setWebviewState).toHaveBeenCalledWith(ID, '{"zoom":2}')

    vi.mocked(ipc.webviewStorage).mockResolvedValueOnce('"white"')
    fromPage(page, {
      ferrite: 'webview',
      type: 'storage',
      data: { id: 7, scope: 'global', op: 'get', key: 'background' },
    })
    expect(ipc.webviewStorage).toHaveBeenCalledWith(ID, 'global', 'get', 'background', null)
    await waitFor(() =>
      expect(posted).toHaveBeenCalledWith(
        { ferrite: 'webview', type: 'storageResult', data: { id: 7, value: '"white"' } },
        '*',
      ),
    )

    vi.mocked(ipc.webviewStorage).mockRejectedValueOnce(new Error('no folder is open'))
    fromPage(page, {
      ferrite: 'webview',
      type: 'storage',
      data: { id: 8, scope: 'workspace', op: 'set', key: 'zoom', value: '2' },
    })
    await waitFor(() =>
      expect(posted).toHaveBeenCalledWith(
        { ferrite: 'webview', type: 'storageResult', data: { id: 8, error: 'no folder is open' } },
        '*',
      ),
    )
    // Malformed requests are ignored.
    fromPage(page, { ferrite: 'webview', type: 'storage', data: { id: 'x', scope: 1 } })
    expect(ipc.webviewStorage).toHaveBeenCalledTimes(2)
  })

  it('gives the page the keybindings, and runs the shortcuts it passes on', async () => {
    const unregister = registerCommands([
      { id: 'test.quickOpen', title: 'Quick Open', keybinding: 'ctrl+p', run: vi.fn() },
    ])
    render(<WebviewFrame id={ID} title="cat.svg" />)
    const frame = await screen.findByTitle<HTMLIFrameElement>('cat.svg')
    const page = frame.contentWindow!
    const posted = vi.spyOn(page, 'postMessage')
    fromPage(page, { ferrite: 'webview', type: 'ready' })
    expect(posted).toHaveBeenCalledWith(
      {
        ferrite: 'webview',
        type: 'keybindings',
        data: expect.arrayContaining([
          { key: 'p', ctrl: true, meta: false, alt: false, shift: false },
        ]),
      },
      '*',
    )

    // As if pressed on the frame, where the keybinding handler hears it.
    const pressed = vi.fn()
    window.addEventListener('keydown', pressed, true)
    const press = { key: 'p', code: 'KeyP', ctrlKey: true, shiftKey: false, altKey: false }
    fromPage(page, {
      ferrite: 'webview',
      type: 'keydown',
      data: { ...press, metaKey: false, repeat: false },
    })
    fromPage(page, { ferrite: 'webview', type: 'keydown', data: { ...press, metaKey: 'no' } })
    expect(pressed).toHaveBeenCalledTimes(1)
    const event = pressed.mock.calls[0]![0] as KeyboardEvent
    expect([event.key, event.ctrlKey, event.target]).toEqual(['p', true, frame])
    window.removeEventListener('keydown', pressed, true)

    fromPage(page, {
      ferrite: 'webview',
      type: 'actionState',
      data: { action: 'fit', state: { pressed: true } },
    })
    expect(webviews.webviewActionStates.get()[ID]).toEqual({ fit: { pressed: true } })
    unregister()
  })
})
