import { describe, expect, it } from 'vitest'
import { nextZoom } from './zoom'

describe('nextZoom', () => {
  it('steps through the zoom levels, stopping at the ends', () => {
    expect(nextZoom(1, 1)).toBe(1.1)
    expect(nextZoom(1, -1)).toBe(0.9)
    expect(nextZoom(3, 1)).toBe(3)
    expect(nextZoom(0.5, -1)).toBe(0.5)
  })

  it('moves a zoom between steps to the nearest step that way', () => {
    expect(nextZoom(1.2, 1)).toBe(1.25)
    expect(nextZoom(1.2, -1)).toBe(1.1)
  })
})
