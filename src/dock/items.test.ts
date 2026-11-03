import { describe, expect, it } from 'vitest'
import type { ExtensionStatus, PreviewInfo } from '../ipc/extensions'
import { keepsLoaded, type ResolvedItem } from './items'

const webview = (retainWhenHidden: boolean) => ({
  page: 'page.html',
  onMessage: null,
  retainWhenHidden,
})

const extension = {
  contributions: {
    viewers: [
      { fullId: 'x.kept', webview: webview(true) },
      { fullId: 'x.reloaded', webview: webview(false) },
      { fullId: 'x.content', webview: null },
    ],
    previews: [{ fullId: 'x.graph', webview: webview(true) }],
  },
} as unknown as ExtensionStatus

const viewer = (id: string): ResolvedItem => ({
  kind: 'viewer',
  item: `viewer:${id}:/a.svg`,
  viewer: id,
  path: '/a.svg',
})

const preview = (id: string, isWebview: boolean): ResolvedItem => ({
  kind: 'preview',
  item: `preview:${id}`,
  preview: { id, webview: isWebview } as PreviewInfo,
})

describe('keepsLoaded', () => {
  it('keeps only webviews whose extension asks for it', () => {
    expect(keepsLoaded(viewer('x.kept'), [extension])).toBe(true)
    expect(keepsLoaded(viewer('x.reloaded'), [extension])).toBe(false)
    expect(keepsLoaded(viewer('x.content'), [extension])).toBe(false)
    expect(keepsLoaded(viewer('x.kept'), [])).toBe(false)
    expect(keepsLoaded(preview('x.graph/key/with/slashes', true), [extension])).toBe(true)
    expect(keepsLoaded(preview('x.graph/k', false), [extension])).toBe(false)
  })
})
