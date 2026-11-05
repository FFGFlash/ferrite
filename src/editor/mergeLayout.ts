/**
 * Lining up a merge tab's three editors: the incoming and current versions
 * above, the result below. Each range the sides changed is one block, as
 * is each change made to the result away from them; every block takes the
 * same height in every editor (a row for its buttons, then its lines, then
 * filler up to the tallest), so lines between blocks line up and the
 * editors scroll together, as VS Code's merge editor does.
 */
import type { MergeInfo, MergeRange, MergeRangeState } from '../ipc/editor'

export type Pane = 'incoming' | 'current' | 'result'
export const PANES: readonly Pane[] = ['incoming', 'current', 'result']

export interface MergeBlock {
  /** The range's index, or `null` for a change the result made elsewhere. */
  range: number | null
  lines: Record<Pane, [number, number]>
}

/** The blocks, in order. */
export function mergeBlocks(info: MergeInfo): MergeBlock[] {
  const blocks: (MergeBlock & { base: number })[] = info.ranges.map((range, index) => ({
    range: index,
    base: range.base[0],
    lines: { incoming: range.incoming, current: range.current, result: range.result },
  }))
  // A change elsewhere is at its base lines in the versions, which moved by
  // what the ranges before it added or removed.
  for (const edit of info.edits) {
    const before = info.ranges.filter(range => range.base[1] <= edit.base[0])
    const shift = (side: 'incoming' | 'current') =>
      before.reduce(
        (sum, range) => sum + (range[side][1] - range[side][0]) - (range.base[1] - range.base[0]),
        0,
      )
    const at = (side: 'incoming' | 'current'): [number, number] => [
      edit.base[0] + shift(side),
      edit.base[1] + shift(side),
    ]
    blocks.push({
      range: null,
      base: edit.base[0],
      lines: { incoming: at('incoming'), current: at('current'), result: edit.result },
    })
  }
  return blocks
    .sort((a, b) => a.base - b.base || (a.range === null ? 1 : 0) - (b.range === null ? 1 : 0))
    .map(({ range, lines }) => ({ range, lines }))
}

export type MergeZone =
  /** The row above a range for its buttons. */
  { kind: 'actions'; row: number; range: number } | { kind: 'filler'; row: number; lines: number }

const length = ([start, end]: [number, number]) => end - start

/** Each editor's zones: rows are the line each is below (-1: above the first). */
export function mergeZones(blocks: readonly MergeBlock[]): Record<Pane, MergeZone[]> {
  const zones: Record<Pane, MergeZone[]> = { incoming: [], current: [], result: [] }
  for (const block of blocks) {
    const tallest = Math.max(...PANES.map(pane => length(block.lines[pane])))
    for (const pane of PANES) {
      const [start, end] = block.lines[pane]
      if (block.range !== null) {
        zones[pane].push({ kind: 'actions', row: start - 1, range: block.range })
      }
      const missing = tallest - (end - start)
      if (missing > 0) {
        zones[pane].push({ kind: 'filler', row: end - 1, lines: missing })
      }
    }
  }
  return zones
}

/** Whether `side`'s change is in a range's result. */
export function hasSide(state: MergeRangeState, side: 'incoming' | 'current'): boolean {
  return state === side || state === 'currentIncoming' || state === 'incomingCurrent'
}

/** A range in the result is unresolved: a conflict neither side's change was accepted into. */
export const unresolved = (range: MergeRange): boolean =>
  range.conflicting && range.state === 'base'

/** What accepting a side does from the state a range is in: toggling that side's change. */
export function toggled(range: MergeRange, side: 'incoming' | 'current'): MergeRangeState {
  const other = side === 'incoming' ? 'current' : 'incoming'
  const has = hasSide(range.state, side)
  const hasOther = range.state !== 'manual' && hasSide(range.state, other)
  if (has) {
    return hasOther ? other : 'base'
  }
  if (!range.conflicting || !hasOther) {
    return side
  }
  // Accepting the second side keeps both: the one accepted first, first.
  return other === 'current' ? 'currentIncoming' : 'incomingCurrent'
}

/** A range's result, in words. */
export function stateLabel(range: MergeRange): string {
  switch (range.state) {
    case 'base':
      return range.conflicting ? 'Unresolved conflict' : 'Change not accepted'
    case 'incoming':
      return 'Incoming accepted'
    case 'current':
      return 'Current accepted'
    case 'currentIncoming':
      return 'Both accepted (current first)'
    case 'incomingCurrent':
      return 'Both accepted (incoming first)'
    case 'manual':
      return 'Edited'
  }
}

/** The class of a line of `pane`, if a range covers it. */
export function lineClassOf(
  ranges: readonly MergeRange[],
  pane: Pane,
  line: number,
): string | undefined {
  const range = ranges.find(candidate => {
    const [start, end] = candidate[pane]
    return start <= line && line < end
  })
  if (!range) {
    return undefined
  }
  if (pane === 'result') {
    if (unresolved(range)) {
      return 'merge-unresolved'
    }
    switch (range.state) {
      case 'base':
        return undefined
      case 'manual':
        return 'merge-manual'
      case 'incoming':
        return 'merge-incoming'
      case 'current':
        return 'merge-current'
      default:
        return 'merge-both'
    }
  }
  if (!(pane === 'incoming' ? range.incomingChanged : range.currentChanged)) {
    return undefined
  }
  const accepted = hasSide(range.state, pane) ? ' merge-accepted' : ''
  return `merge-${pane}${range.conflicting ? ' merge-conflict' : ''}${accepted}`
}

/** The next unresolved conflict after the result's `line` (or before it), wrapping around. */
export function nextConflict(
  ranges: readonly MergeRange[],
  line: number,
  backward: boolean,
): MergeRange | null {
  const open = ranges.filter(unresolved)
  const conflicts = open.length > 0 ? open : ranges.filter(range => range.conflicting)
  if (conflicts.length === 0) {
    return null
  }
  if (backward) {
    return [...conflicts].reverse().find(range => range.result[0] < line) ?? conflicts.at(-1)!
  }
  return conflicts.find(range => range.result[0] > line) ?? conflicts[0]!
}
