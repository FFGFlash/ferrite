import { describe, expect, it } from 'vitest'
import { TOKEN_KINDS } from '../ipc/editor'
import { LINK_CLASS, segments } from './segments'

const kind = (name: string) => TOKEN_KINDS.indexOf(name as (typeof TOKEN_KINDS)[number]) + 1

describe('segments', () => {
  // "fn main() {}" with `fn` a keyword and `main` a function.
  const text = 'fn main() {}'
  const spans = [0, 2, kind('keyword'), 3, 7, kind('function')]

  it('splits a line into plain and highlighted runs', () => {
    expect(segments(text, spans)).toEqual([
      { start: 0, end: 2, className: 'tok-keyword' },
      { start: 2, end: 3, className: '' },
      { start: 3, end: 7, className: 'tok-function' },
      { start: 7, end: 12, className: '' },
    ])
  })

  it('draws a link over a token and keeps its color class', () => {
    expect(segments(text, spans, [{ from: 3, to: 7, className: LINK_CLASS }])).toEqual([
      { start: 0, end: 2, className: 'tok-keyword' },
      { start: 2, end: 3, className: '' },
      { start: 3, end: 7, className: `tok-function ${LINK_CLASS}` },
      { start: 7, end: 12, className: '' },
    ])
  })

  it("splits runs at the link's edges", () => {
    expect(segments('let value = 1;', [], [{ from: 4, to: 9, className: LINK_CLASS }])).toEqual([
      { start: 0, end: 4, className: '' },
      { start: 4, end: 9, className: LINK_CLASS },
      { start: 9, end: 14, className: '' },
    ])
    expect(
      segments(text, spans, [{ from: 1, to: 5, className: LINK_CLASS }]).map(s => [
        s.start,
        s.end,
        s.className,
      ]),
    ).toEqual([
      [0, 1, 'tok-keyword'],
      [1, 2, `tok-keyword ${LINK_CLASS}`],
      [2, 3, LINK_CLASS],
      [3, 5, `tok-function ${LINK_CLASS}`],
      [5, 7, 'tok-function'],
      [7, 12, ''],
    ])
  })

  it('adds decoration styles to the runs they cover', () => {
    const bold = { fontWeight: 'bold' as const }
    const red = { color: '#ff0000' }
    const result = segments(
      'abcdef',
      [],
      [
        { from: 1, to: 4, style: bold },
        { from: 3, to: 6, style: red },
      ],
    )
    expect(result).toEqual([
      { start: 0, end: 1, className: '' },
      { start: 1, end: 3, className: '', style: bold },
      { start: 3, end: 4, className: '', style: { ...bold, ...red } },
      { start: 4, end: 6, className: '', style: red },
    ])
  })
})
