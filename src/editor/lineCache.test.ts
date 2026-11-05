import { describe, expect, it } from 'vitest'
import type { LinesUpdate, WireView } from '../ipc/editor'
import { applyUpdate, knownLines, type LineCache } from './lineCache'

function wire(
  version: number,
  startLine: number,
  endLine: number,
  lines: LinesUpdate,
  visible: [number, number][] = [[startLine, endLine]],
): WireView {
  return {
    documentId: 1,
    version,
    lineCount: 1000,
    rowCount: 1000,
    startRow: startLine,
    startLine,
    endLine,
    visible,
    folded: [],
    foldable: [],
    foldGeneration: 0,
    lines,
    selections: [],
    selectionCount: 1,
    primary: { position: { line: 0, column: 0 }, visualColumn: 0, row: 0 },
    dirty: false,
    search: null,
    highlights: [],
    diagnostics: [],
    language: null,
    decorations: [],
    changes: [],
    lenses: [],
    annotations: [],
  }
}

const texts = (cache: LineCache | null) => cache?.lines.map(line => line.text)

const initial = applyUpdate(
  null,
  wire(1, 0, 5, {
    base: null,
    splices: [],
    chunks: [{ start: 0, lines: ['a', 'b', 'c', 'd', 'e'] }],
  }),
)

describe('applyUpdate', () => {
  it('fills an empty cache from a full update', () => {
    expect(texts(initial)).toEqual(['a', 'b', 'c', 'd', 'e'])
    expect(knownLines(initial)).toEqual({ version: 1, ranges: [[0, 5]] })
  })

  it('replaces only an edited line', () => {
    const next = applyUpdate(
      initial,
      wire(2, 0, 5, {
        base: 1,
        splices: [{ start: 2, removed: 1, inserted: 1 }],
        chunks: [{ start: 2, lines: ['C'] }],
      }),
    )
    expect(texts(next)).toEqual(['a', 'b', 'C', 'd', 'e'])
  })

  it('shifts held lines below an inserted line', () => {
    const next = applyUpdate(
      initial,
      wire(2, 0, 5, {
        base: 1,
        splices: [{ start: 1, removed: 1, inserted: 2 }],
        chunks: [{ start: 1, lines: ['b1', 'b2'] }],
      }),
    )
    expect(texts(next)).toEqual(['a', 'b1', 'b2', 'c', 'd'])
  })

  it('keeps overlapping lines when scrolling and adds new ones', () => {
    const next = applyUpdate(
      initial,
      wire(1, 3, 7, { base: 1, splices: [], chunks: [{ start: 5, lines: ['f', 'g'] }] }),
    )
    expect(texts(next)).toEqual(['d', 'e', 'f', 'g'])
    expect(next?.lineNumbers).toEqual([3, 4, 5, 6])
  })

  it('holds only the visible lines around folded regions', () => {
    const folded = applyUpdate(
      initial,
      wire(1, 0, 7, { base: 1, splices: [], chunks: [{ start: 5, lines: ['f', 'g'] }] }, [
        [0, 2],
        [4, 7],
      ]),
    )
    expect(texts(folded)).toEqual(['a', 'b', 'e', 'f', 'g'])
    expect(folded?.lineNumbers).toEqual([0, 1, 4, 5, 6])
    expect(knownLines(folded)).toEqual({
      version: 1,
      ranges: [
        [0, 2],
        [4, 7],
      ],
    })
    // Unfolding needs the hidden lines, which the backend sends.
    const unfolded = applyUpdate(
      folded,
      wire(1, 0, 5, { base: 1, splices: [], chunks: [{ start: 2, lines: ['c', 'd'] }] }),
    )
    expect(texts(unfolded)).toEqual(['a', 'b', 'c', 'd', 'e'])
  })

  it('carries the full length of shortened lines', () => {
    const next = applyUpdate(
      null,
      wire(1, 0, 1, {
        base: null,
        splices: [],
        chunks: [{ start: 0, lines: [{ text: 'abc', fullLength: 1_000_000 }] }],
      }),
    )
    expect(next?.lines[0]).toEqual({ text: 'abc', fullLength: 1_000_000, spans: [] })
  })

  it('keeps the spans of held lines and applies changed ones', () => {
    const highlighted = applyUpdate(initial, {
      ...wire(1, 0, 5, { base: 1, splices: [], chunks: [] }),
      highlights: [
        { line: 0, spans: [0, 1, 2] },
        { line: 1, spans: [0, 1, 3] },
      ],
    })
    // Line 1 is edited (and so resent without spans); line 0 shifts down.
    const next = applyUpdate(highlighted, {
      ...wire(2, 0, 6, {
        base: 1,
        splices: [{ start: 1, removed: 1, inserted: 2 }],
        chunks: [{ start: 0, lines: ['z', 'b1', 'b2'] }],
      }),
      highlights: [{ line: 3, spans: [0, 1, 1] }],
    })
    expect(next?.lines.map(line => line.spans)).toEqual([[], [], [], [0, 1, 1], [], []])
    const shifted = applyUpdate(highlighted, {
      ...wire(2, 0, 5, { base: 1, splices: [{ start: 0, removed: 0, inserted: 0 }], chunks: [] }),
      highlights: [],
    })
    expect(shifted?.lines[0]?.spans).toEqual([0, 1, 2])
  })

  it('refuses updates it cannot apply', () => {
    // The update is relative to a version the cache does not have.
    expect(applyUpdate(initial, wire(3, 0, 5, { base: 2, splices: [], chunks: [] }))).toBeNull()
    // A line the cache does not hold was not sent.
    expect(applyUpdate(initial, wire(1, 0, 6, { base: 1, splices: [], chunks: [] }))).toBeNull()
  })
})
