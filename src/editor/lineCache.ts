/**
 * The lines an editor holds, updated incrementally from backend views.
 *
 * The cache covers exactly the visible lines of the last viewport (lines in
 * folded regions are left out). Each update replays the line splices since
 * the cached version (dropping edited lines and shifting the rest), then
 * fills in the lines the backend sent. The backend sends only lines the
 * cache cannot provide, so both sides must agree; if they ever don't,
 * `applyUpdate` returns `null` and the caller starts over.
 *
 * Syntax highlights follow the same rule: held lines keep their spans, sent
 * lines start without, and the view lists the lines whose spans changed.
 */
import type { DisplayLine, KnownLines, WireLine, WireView } from '../ipc/editor'

export interface LineCache {
  version: number
  /** The document line each of `lines` is, in order. */
  lineNumbers: number[]
  lines: DisplayLine[]
}

export function knownLines(cache: LineCache | null): KnownLines | null {
  if (!cache) {
    return null
  }
  const ranges: [number, number][] = []
  for (const line of cache.lineNumbers) {
    const last = ranges.at(-1)
    if (last && last[1] === line) {
      last[1] = line + 1
    } else {
      ranges.push([line, line + 1])
    }
  }
  return { version: cache.version, ranges }
}

const NO_SPANS: readonly number[] = []

function toDisplay(line: WireLine): DisplayLine {
  return typeof line === 'string'
    ? { text: line, fullLength: null, spans: NO_SPANS }
    : { text: line.text, fullLength: line.fullLength, spans: NO_SPANS }
}

/** The cache after `view`, or `null` if `view` cannot be applied to it. */
export function applyUpdate(cache: LineCache | null, view: WireView): LineCache | null {
  const { base, splices, chunks } = view.lines
  const lines = new Map<number, DisplayLine>()

  if (base !== null) {
    if (!cache || cache.version !== base) {
      return null
    }
    let held: [number, DisplayLine][] = cache.lines.map((line, i) => [cache.lineNumbers[i]!, line])
    for (const splice of splices) {
      const removedEnd = splice.start + splice.removed
      const shift = splice.inserted - splice.removed
      held = held
        .filter(([line]) => line < splice.start || line >= removedEnd)
        .map(([line, text]) => [line >= removedEnd ? line + shift : line, text])
    }
    for (const [line, text] of held) {
      lines.set(line, text)
    }
  }
  for (const chunk of chunks) {
    chunk.lines.forEach((line, i) => lines.set(chunk.start + i, toDisplay(line)))
  }

  const resolved = new Map<number, DisplayLine>()
  for (const [start, end] of view.visible) {
    for (let line = start; line < end; line++) {
      const text = lines.get(line)
      if (text === undefined) {
        return null
      }
      resolved.set(line, text)
    }
  }
  for (const { line, spans } of view.highlights) {
    const held = resolved.get(line)
    if (held) {
      resolved.set(line, { ...held, spans })
    }
  }
  return {
    version: view.version,
    lineNumbers: [...resolved.keys()],
    lines: [...resolved.values()],
  }
}
