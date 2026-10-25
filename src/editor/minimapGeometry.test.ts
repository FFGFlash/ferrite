import { describe, expect, it } from 'vitest'
import { dragScale, minimapLayout, scrollTopFor } from './minimapGeometry'

// 20px lines in the editor, 2px in the minimap: a ratio of 1/10.
const lineHeight = 20

describe('minimap geometry', () => {
  it("doesn't scroll when the whole document fits", () => {
    // 100 lines plus a view less one line: 2,000 + 380 px, 238 px in the minimap.
    const scroll = { scrollTop: 600, scrollHeight: 2380, clientHeight: 400 }
    const layout = minimapLayout(scroll, lineHeight, 300)
    expect(layout.offset).toBe(0)
    expect(layout.sliderTop).toBe(60)
    expect(layout.sliderHeight).toBe(40)
    expect(layout.firstLine).toBe(0)
    expect(layout.lineCount).toBe(151)
    expect(dragScale(scroll, lineHeight, 300)).toBe(10)
  })

  it('scrolls in proportion when the document is taller', () => {
    // 10,000 lines: 200,000 px of lines, 20,038 px in the minimap.
    const top = { scrollTop: 0, scrollHeight: 200_380, clientHeight: 400 }
    expect(minimapLayout(top, lineHeight, 300)).toMatchObject({ offset: 0, sliderTop: 0 })

    const bottom = { ...top, scrollTop: 199_980 }
    const end = minimapLayout(bottom, lineHeight, 300)
    expect(end.offset).toBeCloseTo(20_038 - 300)
    // At the bottom, the slider sits at the bottom of the minimap.
    expect(end.sliderTop + end.sliderHeight).toBeCloseTo(300)
    expect(end.firstLine).toBe(Math.floor(end.offset / 2))

    const middle = minimapLayout({ ...top, scrollTop: 99_990 }, lineHeight, 300)
    expect(middle.sliderTop).toBeCloseTo((300 - 40) / 2)

    // Dragging the slider across its travel scrolls the whole document.
    expect(dragScale(top, lineHeight, 300) * (300 - 40)).toBeCloseTo(199_980)
  })

  it('clamps scroll positions outside the scrollable range', () => {
    const scroll = { scrollTop: -50, scrollHeight: 2380, clientHeight: 400 }
    expect(minimapLayout(scroll, lineHeight, 300).sliderTop).toBe(0)
  })

  it('centers the clicked line', () => {
    const scroll = { scrollTop: 0, scrollHeight: 2380, clientHeight: 400 }
    const layout = minimapLayout(scroll, lineHeight, 300)
    // y = 101 is line 50, which starts at 1,000 px.
    expect(scrollTopFor(101, layout, scroll, lineHeight)).toBe(1000 + 10 - 200)

    const tall = { scrollTop: 199_980, scrollHeight: 200_380, clientHeight: 400 }
    const scrolled = minimapLayout(tall, lineHeight, 300)
    const line = Math.floor((10 + scrolled.offset) / 2)
    expect(scrollTopFor(10, scrolled, tall, lineHeight)).toBe(line * 20 + 10 - 200)
  })
})
