/**
 * Laying out a diff: which unchanged lines collapse, and the room each side
 * leaves so lines line up. Line ranges are `[start, end)`.
 *
 * Side by side, a change with fewer lines on one side gets filler below its
 * last line there (or above the first line, for a change at the top), so the
 * lines after it are level. Collapsed lines are folded on both sides (an
 * unchanged stretch has as many lines on each), with a bar below the line
 * before them; the fold keeps that line showing.
 */
import type { LineChange } from '../ipc/editor'

/** Unchanged lines kept around a change when the others collapse. */
export const CONTEXT_LINES = 3
/** Fewer unchanged lines than this don't collapse. */
export const MIN_COLLAPSED = 3

export type Side = 'original' | 'modified'

/** Unchanged lines hidden on both sides: as many on each. */
export interface Collapsed {
  /** The unchanged stretch's first original line, which names it. */
  key: number
  original: [number, number]
  modified: [number, number]
}

/**
 * The unchanged lines to collapse, leaving `CONTEXT_LINES` next to each
 * change, the first line of the file, and the stretches in `revealed` (by
 * key).
 */
export function collapsedRegions(
  changes: readonly LineChange[],
  /** The modified side's; after the last change, both sides have as many lines. */
  lineCount: number,
  revealed: ReadonlySet<number>,
): Collapsed[] {
  const regions: Collapsed[] = []
  const add = (
    original: number,
    modified: number,
    length: number,
    first: boolean,
    last: boolean,
  ) => {
    // The line before hidden lines stays, so the first line always does.
    const from = Math.max(first ? 0 : CONTEXT_LINES, original === 0 ? 1 : 0)
    const to = length - (last ? 0 : CONTEXT_LINES)
    if (to - from >= MIN_COLLAPSED && !revealed.has(original)) {
      regions.push({
        key: original,
        original: [original + from, original + to],
        modified: [modified + from, modified + to],
      })
    }
  }
  let original = 0
  let modified = 0
  changes.forEach((change, index) => {
    add(original, modified, change.original[0] - original, index === 0, false)
    original = change.original[1]
    modified = change.modified[1]
  })
  add(original, modified, lineCount - modified, changes.length === 0, true)
  return regions
}

/** The fold regions (`[start, end)`, `start` showing) hiding a side's collapsed lines. */
export function foldsFor(collapsed: readonly Collapsed[], side: Side): [number, number][] {
  return collapsed.map(region => [region[side][0] - 1, region[side][1]])
}

/** The display row of `line` when `hidden` ranges (in order) are folded away. */
export function rowOfLine(hidden: readonly (readonly [number, number])[], line: number): number {
  let row = line
  for (const [start, end] of hidden) {
    if (start > line) {
      break
    }
    row -= Math.min(line + 1, end) - start
    if (line < end) {
      // A hidden line is on the row of the line before.
      return row
    }
  }
  return row
}

/** A zone a diff adds to one side's editor. */
export type DiffZone =
  /** Room for lines only the other side has. */
  | { kind: 'filler'; row: number; lines: number }
  /** The bar standing for collapsed lines. */
  | { kind: 'collapsed'; row: number; lines: 1; region: Collapsed }
  /** Removed lines, shown in an inline diff: `change` indexes the changes. */
  | { kind: 'removed'; row: number; lines: number; change: number }

function collapsedZones(collapsed: readonly Collapsed[], side: Side): DiffZone[] {
  const hidden = collapsed.map(region => region[side])
  return collapsed.map(region => ({
    kind: 'collapsed',
    row: rowOfLine(hidden, region[side][0] - 1),
    lines: 1,
    region,
  }))
}

/** Each side's zones in a side-by-side diff. */
export function sideBySideZones(
  changes: readonly LineChange[],
  collapsed: readonly Collapsed[],
): Record<Side, DiffZone[]> {
  const zones: Record<Side, DiffZone[]> = {
    original: collapsedZones(collapsed, 'original'),
    modified: collapsedZones(collapsed, 'modified'),
  }
  for (const change of changes) {
    const removed = change.original[1] - change.original[0]
    const inserted = change.modified[1] - change.modified[0]
    const [side, lines] =
      removed < inserted
        ? (['original', inserted - removed] as const)
        : (['modified', removed - inserted] as const)
    if (lines > 0) {
      const hidden = collapsed.map(region => region[side])
      zones[side].push({ kind: 'filler', row: rowOfLine(hidden, change[side][1] - 1), lines })
    }
  }
  return zones
}

/** The modified side's zones in an inline diff: its removed lines and collapsed bars. */
export function inlineZones(
  changes: readonly LineChange[],
  collapsed: readonly Collapsed[],
): DiffZone[] {
  const zones = collapsedZones(collapsed, 'modified')
  const hidden = collapsed.map(region => region.modified)
  changes.forEach((change, index) => {
    const lines = change.original[1] - change.original[0]
    if (lines > 0) {
      zones.push({
        kind: 'removed',
        row: rowOfLine(hidden, change.modified[0] - 1),
        lines,
        change: index,
      })
    }
  })
  return zones
}

/** The change with `line` among its lines on `side`, if any. */
export function changeAtLine(
  changes: readonly LineChange[],
  side: Side,
  line: number,
): LineChange | null {
  let low = 0
  let high = changes.length
  while (low < high) {
    const middle = (low + high) >> 1
    if (changes[middle]![side][1] <= line) {
      low = middle + 1
    } else {
      high = middle
    }
  }
  const change = changes[low]
  return change && change[side][0] <= line ? change : null
}

/**
 * The change to go to from `line` on the modified side: the next one after
 * it, or (`backward`) the one before it, wrapping around.
 */
export function nextChange(
  changes: readonly LineChange[],
  line: number,
  backward: boolean,
): LineChange | null {
  if (changes.length === 0) {
    return null
  }
  if (backward) {
    return [...changes].reverse().find(change => change.modified[0] < line) ?? changes.at(-1)!
  }
  return changes.find(change => change.modified[0] > line) ?? changes[0]!
}
