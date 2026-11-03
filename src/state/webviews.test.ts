import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('../ipc/extensions', () => ({ setWebviewTheme: vi.fn(async () => {}) }))

const ipc = await import('../ipc/extensions')
const { applyTheme } = await import('../themes/theme')
const webviews = await import('./webviews')

const IMAGES = 'example.images.svg//w/a.svg'
const OTHER_PAGE = 'example.images.graph/key'
const ELSEWHERE = 'example.imagesx.svg//w/b.svg'

describe('webviews', () => {
  beforeEach(() => {
    for (const id of [IMAGES, OTHER_PAGE, ELSEWHERE]) {
      webviews.webviewClosed(id)
    }
  })

  it('keeps posts for a page until it loads', () => {
    webviews.postToWebview(IMAGES, 'action', 'fit')
    webviews.webviewMessage(IMAGES, '{"a":1}')
    const post = vi.fn()
    const stop = webviews.registerWebviewFrame(IMAGES, post)
    expect(post.mock.calls).toEqual([
      ['action', 'fit'],
      ['message', '{"a":1}'],
    ])
    webviews.postToWebview(IMAGES, 'action', 'zoom-in')
    expect(post).toHaveBeenLastCalledWith('action', 'zoom-in')
    stop()
  })

  it('tells an extension’s other loaded pages that a stored value changed', () => {
    const [images, other, elsewhere] = [vi.fn(), vi.fn(), vi.fn()]
    const stops = [
      webviews.registerWebviewFrame(IMAGES, images),
      webviews.registerWebviewFrame(OTHER_PAGE, other),
      webviews.registerWebviewFrame(ELSEWHERE, elsewhere),
    ]
    webviews.storageChanged('example.images', 'global', 'background', '"white"', IMAGES)
    expect(images).not.toHaveBeenCalled()
    expect(other).toHaveBeenCalledWith('storageChanged', {
      scope: 'global',
      key: 'background',
      value: '"white"',
    })
    expect(elsewhere).not.toHaveBeenCalled()
    // From the extension's code: every page hears of it.
    webviews.storageChanged('example.images', null, null, null, null)
    expect(images).toHaveBeenCalledWith('storageChanged', { scope: null, key: null, value: null })
    stops.forEach(stop => stop())
  })

  it('keeps only the action state fields it knows', () => {
    webviews.setActionState(IMAGES, 'fit', { pressed: true, title: 'Fit', color: 'red' })
    webviews.setActionState(IMAGES, 'fit', { disabled: 'yes', hidden: false })
    webviews.setActionState(IMAGES, 'grid', null)
    expect(webviews.webviewActionStates.get()[IMAGES]).toEqual({
      fit: { pressed: true, title: 'Fit', hidden: false },
    })
    webviews.webviewClosed(IMAGES)
    expect(webviews.webviewActionStates.get()[IMAGES]).toBeUndefined()
  })

  it('gives pages the theme when it changes', () => {
    const post = vi.fn()
    const stop = webviews.registerWebviewFrame(IMAGES, post)
    document.documentElement.style.setProperty('--color-editor-background', '#123456')
    applyTheme('light')
    const theme = webviews.currentWebviewTheme()
    expect(theme?.kind).toBe('light')
    expect(theme?.variables['editor-background']).toBe('#123456')
    expect(ipc.setWebviewTheme).toHaveBeenCalledWith('light', theme?.variables)
    expect(post).toHaveBeenCalledWith('theme', theme)
    stop()
  })
})
