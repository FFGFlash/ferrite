import { describe, expect, it } from 'vitest'
import { DEFAULT_LAYOUT, parseLayout } from './layout'

describe('parseLayout', () => {
  it('restores valid saved values', () => {
    const dock = { main: 'main', root: { kind: 'group', id: 'main', items: [], active: null } }
    expect(parseLayout({ expanded: ['/a'], minimapVisible: false, zoom: 1.25, dock })).toEqual({
      expanded: ['/a'],
      minimapVisible: false,
      zoom: 1.25,
      dock,
    })
  })

  it('falls back to defaults for missing or invalid fields', () => {
    expect(parseLayout({})).toEqual(DEFAULT_LAYOUT)
    expect(parseLayout({ expanded: [1, '/b'], minimapVisible: 'no', zoom: 40 })).toEqual({
      ...DEFAULT_LAYOUT,
      expanded: ['/b'],
    })
  })

  it('keeps the side bar settings of layouts from before the dock', () => {
    expect(
      parseLayout({ sidebarVisible: false, activeView: 'search', sidebarWidth: 300 }).dock,
    ).toEqual({ sidebarVisible: false, activeView: 'search' })
  })

  it('treats anything but an object as no saved layout', () => {
    expect(parseLayout(null)).toEqual(DEFAULT_LAYOUT)
    expect(parseLayout([1, 2])).toEqual(DEFAULT_LAYOUT)
    expect(parseLayout('layout')).toEqual(DEFAULT_LAYOUT)
  })
})
