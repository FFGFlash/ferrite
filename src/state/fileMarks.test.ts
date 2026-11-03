import { describe, expect, it } from 'vitest'
import type { FileMark } from '../ipc/extensions'
import { colorResolver, indexMarks, rowMark } from './fileMarks'

const mark = (path: string, color: string, priority: number): FileMark => ({
  path,
  badge: color[0]?.toUpperCase() ?? null,
  color: `x.git.${color}`,
  tooltip: color,
  faded: false,
  inherit: false,
  propagate: true,
  priority,
})

const color = colorResolver(
  new Map([
    ['x.git.modified', { dark: '#111111', light: '#aaaaaa' }],
    ['x.git.conflict', { dark: '#222222', light: '#bbbbbb' }],
  ]),
  {},
  'dark',
)

describe('explorer marks', () => {
  it('give folders the strongest color inside them, whatever the order', () => {
    for (const marks of [
      [mark('/w/a/b/one.rs', 'modified', 3), mark('/w/a/two.rs', 'conflict', 5)],
      [mark('/w/a/two.rs', 'conflict', 5), mark('/w/a/b/one.rs', 'modified', 3)],
    ]) {
      const index = indexMarks([{ extension: 'x.git', marks }])
      expect(rowMark(index, '/w/a', true, color)?.color).toBe('#222222')
      expect(rowMark(index, '/w', true, color)?.color).toBe('#222222')
      expect(rowMark(index, '/w/a/b', true, color)?.color).toBe('#111111')
      // Files get only their own marks.
      expect(rowMark(index, '/w/a/b/one.rs', false, color)?.badge).toBe('M')
      expect(rowMark(index, '/w/a/three.rs', false, color)).toBeNull()
    }
  })

  it("combine several extensions' marks on one path", () => {
    const index = indexMarks([
      { extension: 'x.git', marks: [mark('/w/f', 'modified', 3)] },
      { extension: 'x.lint', marks: [{ ...mark('/w/f', 'conflict', 1), badge: 'L' }] },
    ])
    expect(rowMark(index, '/w/f', false, color)).toEqual({
      badge: 'M, L',
      color: '#111111',
      tooltip: 'modified\nconflict',
      faded: false,
    })
  })

  it("use the theme's or the user's colors over the extension's", () => {
    const declared = new Map([['x.git.modified', { dark: '#111111', light: '#aaaaaa' }]])
    expect(colorResolver(declared, {}, 'light')('x.git.modified')).toBe('#aaaaaa')
    expect(
      colorResolver(declared, { 'x.git.modified': '#ff0000' }, 'light')('x.git.modified'),
    ).toBe('#ff0000')
    expect(colorResolver(declared, {}, 'dark')('x.git.unknown')).toBeNull()
  })
})
