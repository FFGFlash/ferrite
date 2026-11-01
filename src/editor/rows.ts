/**
 * Finding lines among the rows of a view. A view's rows are its visible
 * lines in order (`lineNumbers`); lines in folded regions have no row.
 */

/** The index of the first of `lineNumbers` (sorted) at or after `line`. */
export function rowAtOrAfter(lineNumbers: readonly number[], line: number): number {
  let low = 0
  let high = lineNumbers.length
  while (low < high) {
    const middle = (low + high) >> 1
    if (lineNumbers[middle]! < line) {
      low = middle + 1
    } else {
      high = middle
    }
  }
  return low
}

/** The index of `line` in `lineNumbers`, or -1 if it has no row there. */
export function rowOf(lineNumbers: readonly number[], line: number): number {
  const row = rowAtOrAfter(lineNumbers, line)
  return lineNumbers[row] === line ? row : -1
}

interface Rows {
  lineNumbers: readonly number[]
  lineCount: number
  rowCount: number
  startRow: number
}

/**
 * The line on `row` (counted from the view's first row). Rows outside the
 * view are guessed as if nothing beyond it were folded.
 */
export function lineAtRow(view: Rows, row: number): number {
  const { lineNumbers } = view
  if (row < 0) {
    return Math.max(0, (lineNumbers[0] ?? 0) + row)
  }
  if (row >= lineNumbers.length) {
    return Math.min(view.lineCount - 1, (lineNumbers.at(-1) ?? 0) + row - lineNumbers.length + 1)
  }
  return lineNumbers[row]!
}

/**
 * The display row of `line`: exact in the view (a hidden line is on its
 * region's first row), guessed outside it.
 */
export function displayRow(view: Rows, line: number): number {
  const { lineNumbers } = view
  const first = lineNumbers[0] ?? 0
  const last = lineNumbers.at(-1) ?? 0
  if (line < first) {
    return Math.min(line, view.startRow)
  }
  if (line > last) {
    return Math.max(view.startRow + lineNumbers.length, line - (view.lineCount - view.rowCount))
  }
  const row = rowAtOrAfter(lineNumbers, line)
  return view.startRow + (lineNumbers[row] === line ? row : row - 1)
}
