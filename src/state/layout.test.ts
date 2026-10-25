import { describe, expect, it } from 'vitest'
import { DEFAULT_LAYOUT, SIDEBAR_MAX_WIDTH, parseLayout } from './layout'

describe('parseLayout', () => {
  it('restores valid saved values', () => {
    expect(
      parseLayout({
        sidebarVisible: false,
        sidebarWidth: 300,
        activeView: 'search',
        expanded: ['/a'],
      }),
    ).toEqual({ sidebarVisible: false, sidebarWidth: 300, activeView: 'search', expanded: ['/a'] })
  })

  it('falls back to defaults for missing or invalid fields', () => {
    expect(parseLayout({})).toEqual(DEFAULT_LAYOUT)
    expect(
      parseLayout({
        sidebarVisible: 'yes',
        sidebarWidth: 'wide',
        activeView: 3,
        expanded: [1, '/b'],
      }),
    ).toEqual({ ...DEFAULT_LAYOUT, expanded: ['/b'] })
  })

  it('treats anything but an object as no saved layout', () => {
    expect(parseLayout(null)).toEqual(DEFAULT_LAYOUT)
    expect(parseLayout([1, 2])).toEqual(DEFAULT_LAYOUT)
    expect(parseLayout('layout')).toEqual(DEFAULT_LAYOUT)
  })

  it('clamps the side bar width', () => {
    expect(parseLayout({ sidebarWidth: 5000 }).sidebarWidth).toBe(SIDEBAR_MAX_WIDTH)
  })
})
