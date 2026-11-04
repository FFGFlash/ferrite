import { describe, expect, it } from 'vitest'
import type { LineChange } from '../ipc/editor'
import {
  changeAtLine,
  collapsedRegions,
  foldsFor,
  inlineZones,
  nextChange,
  rowOfLine,
  sideBySideZones,
} from './diffLayout'

const change = (original: [number, number], modified: [number, number]): LineChange => ({
  original,
  modified,
  chars: [],
})

describe('collapsedRegions', () => {
  it('collapses unchanged lines away from changes, keeping context', () => {
    // 40 lines; line 20 replaced by two lines.
    const changes = [change([20, 21], [20, 22])]
    const regions = collapsedRegions(changes, 41, new Set())
    expect(regions).toEqual([
      // The first line stays, and three lines before the change.
      { key: 0, original: [1, 17], modified: [1, 17] },
      // Three lines after it, then the rest.
      { key: 21, original: [24, 40], modified: [25, 41] },
    ])
  })

  it('leaves short stretches and revealed ones', () => {
    const changes = [change([10, 11], [10, 11]), change([16, 17], [16, 17])]
    // Between the changes, 5 unchanged lines are all context; after the
    // last, 1 line is too few.
    expect(collapsedRegions(changes, 18, new Set()).map(r => r.key)).toEqual([0])
    expect(collapsedRegions(changes, 18, new Set([0]))).toEqual([])
  })

  it('collapses all but the first line of an unchanged file', () => {
    expect(collapsedRegions([], 10, new Set())).toEqual([
      { key: 0, original: [1, 10], modified: [1, 10] },
    ])
  })

  it('folds from the line before the hidden lines', () => {
    const regions = collapsedRegions([], 10, new Set())
    expect(foldsFor(regions, 'modified')).toEqual([[0, 10]])
  })
})

describe('rowOfLine', () => {
  it('counts rows without hidden lines', () => {
    const hidden: [number, number][] = [
      [2, 5],
      [8, 10],
    ]
    expect(rowOfLine(hidden, 1)).toBe(1)
    expect(rowOfLine(hidden, 5)).toBe(2)
    expect(rowOfLine(hidden, 3)).toBe(1)
    expect(rowOfLine(hidden, 10)).toBe(5)
    expect(rowOfLine(hidden, -1)).toBe(-1)
  })
})

describe('sideBySideZones', () => {
  it('fills the shorter side of each change', () => {
    const changes = [
      // Two lines became four.
      change([2, 4], [2, 6]),
      // A line inserted at the top of the other side... and one removed.
      change([10, 11], [12, 12]),
    ]
    const zones = sideBySideZones(changes, [])
    expect(zones.original).toEqual([{ kind: 'filler', row: 3, lines: 2 }])
    expect(zones.modified).toEqual([{ kind: 'filler', row: 11, lines: 1 }])
  })

  it('puts filler for lines added at the top above the first row', () => {
    const zones = sideBySideZones([change([0, 0], [0, 2])], [])
    expect(zones.original).toEqual([{ kind: 'filler', row: -1, lines: 2 }])
  })

  it('places zones on rows that skip collapsed lines', () => {
    const changes = [change([20, 21], [20, 23])]
    const collapsed = collapsedRegions(changes, 43, new Set())
    const zones = sideBySideZones(changes, collapsed)
    // Rows 0..3 are lines 0 and 17..19; line 20 is row 4, and the bars
    // follow the three lines of context after the change, level with the
    // original's filler.
    expect(zones.original).toContainEqual({ kind: 'filler', row: 4, lines: 2 })
    expect(zones.original.filter(zone => zone.kind === 'collapsed').map(zone => zone.row)).toEqual([
      0, 7,
    ])
    expect(zones.modified.filter(zone => zone.kind === 'collapsed').map(zone => zone.row)).toEqual([
      0, 9,
    ])
  })
})

describe('inlineZones', () => {
  it('shows removed lines above the lines that replaced them', () => {
    const changes = [change([2, 4], [2, 3]), change([6, 6], [5, 7])]
    expect(inlineZones(changes, [])).toEqual([{ kind: 'removed', row: 1, lines: 2, change: 0 }])
  })
})

describe('changes', () => {
  const changes = [change([2, 4], [2, 3]), change([10, 10], [9, 11])]

  it('are found by line', () => {
    expect(changeAtLine(changes, 'original', 3)).toBe(changes[0])
    expect(changeAtLine(changes, 'original', 4)).toBeNull()
    expect(changeAtLine(changes, 'modified', 10)).toBe(changes[1])
    expect(changeAtLine(changes, 'original', 10)).toBeNull()
  })

  it('are visited in turn, wrapping around', () => {
    expect(nextChange(changes, 0, false)).toBe(changes[0])
    expect(nextChange(changes, 2, false)).toBe(changes[1])
    expect(nextChange(changes, 9, false)).toBe(changes[0])
    expect(nextChange(changes, 9, true)).toBe(changes[0])
    expect(nextChange(changes, 2, true)).toBe(changes[1])
    expect(nextChange([], 0, false)).toBeNull()
  })
})
