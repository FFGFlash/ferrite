import { describe, expect, it } from 'vitest'
import type { Reference } from '../ipc/lsp'
import {
  describePath,
  groupReferences,
  nearestReference,
  referencePreview,
  stepReference,
} from './references'

function reference(path: string, line: number, column: number, text = ''): Reference {
  return {
    path,
    start: { line, column },
    end: { line, column: column + 3 },
    line: text,
  }
}

const references = [
  reference('/w/src/a.rs', 1, 4),
  reference('/w/src/a.rs', 9, 0),
  reference('/w/src/b.rs', 2, 8),
  reference('/w/src/b.rs', 40, 2),
]

describe('references', () => {
  it('groups references by file, in order', () => {
    const groups = groupReferences(references)
    expect(groups.map(group => [group.path, group.references.length])).toEqual([
      ['/w/src/a.rs', 2],
      ['/w/src/b.rs', 2],
    ])
  })

  it('starts on the reference nearest the position, preferring its file', () => {
    const groups = groupReferences(references)
    expect(nearestReference(groups, '/w/src/b.rs', { line: 38, column: 0 })).toEqual({
      group: 1,
      index: 1,
    })
    expect(nearestReference(groups, '/w/src/a.rs', { line: 38, column: 0 })).toEqual({
      group: 0,
      index: 1,
    })
    // On the same line, the closer column wins.
    const sameLine = groupReferences([reference('/x', 3, 2), reference('/x', 3, 20)])
    expect(nearestReference(sameLine, '/x', { line: 3, column: 18 })).toEqual({
      group: 0,
      index: 1,
    })
  })

  it('steps through references across files, wrapping around', () => {
    const groups = groupReferences(references)
    expect(stepReference(groups, { group: 0, index: 1 }, true)).toEqual({ group: 1, index: 0 })
    expect(stepReference(groups, { group: 1, index: 1 }, true)).toEqual({ group: 0, index: 0 })
    expect(stepReference(groups, { group: 0, index: 0 }, false)).toEqual({ group: 1, index: 1 })
  })

  it('previews a reference with a few characters before it, from a word start', () => {
    const line = '    let total = compute_sum(values) + 1;  '
    const preview = referencePreview({
      path: '/x',
      start: { line: 0, column: 16 },
      end: { line: 0, column: 27 },
      line,
    })
    expect(preview).toEqual({
      before: 'total = ',
      inside: 'compute_sum',
      after: '(values) + 1;',
    })
    expect(referencePreview(reference('/x', 0, 0, 'foo()'))).toEqual({
      before: '',
      inside: 'foo',
      after: '()',
    })
  })

  it('describes paths relative to the workspace', () => {
    expect(describePath('/w/src/a.rs', '/w')).toEqual({ name: 'a.rs', folder: 'src' })
    expect(describePath('/w/a.rs', '/w')).toEqual({ name: 'a.rs', folder: '' })
    expect(describePath('C:\\w\\src\\deep\\a.rs', 'C:\\w')).toEqual({
      name: 'a.rs',
      folder: 'src/deep',
    })
    expect(describePath('/elsewhere/a.rs', '/w')).toEqual({ name: 'a.rs', folder: '/elsewhere' })
  })
})
