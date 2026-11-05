import { describe, expect, it } from 'vitest'
import type { MergeInfo, MergeRange } from '../ipc/editor'
import {
  lineClassOf,
  mergeBlocks,
  mergeZones,
  nextConflict,
  stateLabel,
  toggled,
  unresolved,
} from './mergeLayout'

const range = (overrides: Partial<MergeRange>): MergeRange => ({
  base: [0, 0],
  incoming: [0, 0],
  current: [0, 0],
  result: [0, 0],
  conflicting: false,
  incomingChanged: false,
  currentChanged: false,
  state: 'base',
  ...overrides,
})

// Base: 10 lines. Incoming replaced line 2 with three lines; both changed
// line 6 (a conflict), which the result has as the base does; and the
// result changed line 8 by hand.
const conflict = range({
  base: [6, 7],
  incoming: [8, 9],
  current: [6, 8],
  result: [8, 9],
  conflicting: true,
  incomingChanged: true,
  currentChanged: true,
})
const info: MergeInfo = {
  ranges: [
    range({
      base: [2, 3],
      incoming: [2, 5],
      current: [2, 3],
      result: [2, 5],
      incomingChanged: true,
      state: 'incoming',
    }),
    conflict,
  ],
  edits: [{ base: [8, 9], result: [10, 11] }],
  version: 3,
}

describe('merge layout', () => {
  it('makes a block of each range and of each change elsewhere, in order', () => {
    const blocks = mergeBlocks(info)
    expect(blocks.map(block => block.range)).toEqual([0, 1, null])
    // The change elsewhere is where the versions have its base lines.
    expect(blocks[2]!.lines).toEqual({ incoming: [10, 11], current: [9, 10], result: [10, 11] })
  })

  it('gives each block the same height in every editor', () => {
    const zones = mergeZones(mergeBlocks(info))
    const height = (pane: 'incoming' | 'current' | 'result') =>
      zones[pane].reduce((sum, zone) => sum + (zone.kind === 'filler' ? zone.lines : 1), 0)
    // A row of buttons for each range, and filler where a side is shorter.
    expect(height('incoming')).toBe(2 + 1)
    expect(height('current')).toBe(2 + 2)
    expect(height('result')).toBe(2 + 1)
    expect(zones.current).toContainEqual({ kind: 'filler', row: 2, lines: 2 })
    expect(zones.result[0]).toEqual({ kind: 'actions', row: 1, range: 0 })
    // Lines between blocks line up: each editor's line before the conflict
    // is at the same height (rows before it plus zones).
    const top = (pane: 'incoming' | 'current' | 'result', line: number) =>
      line +
      zones[pane]
        .filter(zone => zone.row < line)
        .reduce((sum, zone) => sum + (zone.kind === 'filler' ? zone.lines : 1), 0)
    expect(top('incoming', 8)).toBe(top('current', 6))
    expect(top('incoming', 8)).toBe(top('result', 8))
  })

  it('toggles a side in and out, keeping both for a conflict', () => {
    expect(toggled(conflict, 'incoming')).toBe('incoming')
    expect(toggled({ ...conflict, state: 'current' }, 'incoming')).toBe('currentIncoming')
    expect(toggled({ ...conflict, state: 'incoming' }, 'current')).toBe('incomingCurrent')
    expect(toggled({ ...conflict, state: 'currentIncoming' }, 'current')).toBe('incoming')
    expect(toggled({ ...conflict, state: 'incoming' }, 'incoming')).toBe('base')
    expect(toggled({ ...conflict, state: 'manual' }, 'current')).toBe('current')
    expect(toggled(info.ranges[0]!, 'incoming')).toBe('base')
  })

  it('names and colors what the result has', () => {
    expect(unresolved(conflict)).toBe(true)
    expect(stateLabel(conflict)).toBe('Unresolved conflict')
    expect(lineClassOf(info.ranges, 'result', 8)).toBe('merge-unresolved')
    expect(lineClassOf(info.ranges, 'result', 3)).toBe('merge-incoming')
    expect(lineClassOf(info.ranges, 'incoming', 3)).toBe('merge-incoming merge-accepted')
    expect(lineClassOf(info.ranges, 'current', 6)).toBe('merge-current merge-conflict')
    // Current didn't change the first range.
    expect(lineClassOf(info.ranges, 'current', 2)).toBeUndefined()
    expect(lineClassOf(info.ranges, 'result', 0)).toBeUndefined()
  })

  it('moves between unresolved conflicts, around the ends', () => {
    const second = { ...conflict, result: [20, 21] as [number, number] }
    const ranges = [conflict, second]
    expect(nextConflict(ranges, 0, false)).toBe(conflict)
    expect(nextConflict(ranges, 8, false)).toBe(second)
    expect(nextConflict(ranges, 20, false)).toBe(conflict)
    expect(nextConflict(ranges, 8, true)).toBe(second)
    // Resolved ones are skipped while others aren't.
    expect(nextConflict([{ ...conflict, state: 'current' }, second], 0, false)).toBe(second)
    expect(nextConflict([], 0, false)).toBeNull()
  })
})
