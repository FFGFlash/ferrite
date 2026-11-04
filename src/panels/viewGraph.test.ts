import { describe, expect, it } from 'vitest'
import { graphShape, passingShape } from './viewGraph'

const lane = (id: string, color = 0) => ({ id, color })

describe('graphShape', () => {
  it('ends lanes for the node at it and leaves it for its parent', () => {
    // b, on the first lane, with a's lane beside it.
    const shape = graphShape({
      item: 'b',
      node: 'b',
      parents: ['c'],
      inputs: [lane('b'), lane('x', 1)],
      outputs: [lane('c'), lane('x', 1)],
    })
    expect(shape.node).toEqual({ x: 5.5, color: 0, merge: false })
    expect(shape.lanes).toBe(2)
    expect(shape.paths).toEqual([
      { d: 'M 5.5 0 L 5.5 11', color: 0 },
      { d: 'M 16.5 0 L 16.5 22', color: 1 },
      { d: 'M 5.5 11 L 5.5 22', color: 0 },
    ])
  })

  it('starts a new lane for a branch tip and curves into merges', () => {
    // A tip nothing above leads to: a lane of its own, after the others.
    const tip = graphShape({
      item: 't',
      node: 't',
      parents: ['p'],
      inputs: [lane('a')],
      outputs: [lane('a'), lane('p', 2)],
    })
    expect(tip.node.x).toBe(16.5)
    expect(tip.paths.at(-1)).toEqual({ d: 'M 16.5 11 L 16.5 22', color: 2 })

    // A merge leaves for both parents; the second on a lane to its right.
    const merge = graphShape({
      item: 'm',
      node: 'm',
      parents: ['p1', 'p2'],
      inputs: [lane('m')],
      outputs: [lane('p1'), lane('p2', 1)],
    })
    expect(merge.node.merge).toBe(true)
    expect(merge.paths).toContainEqual({ d: 'M 5.5 11 C 5.5 16.5 16.5 16.5 16.5 22', color: 1 })
  })

  it('joins lanes that lead to the same node', () => {
    const shape = graphShape({
      item: 'c',
      node: 'c',
      parents: [],
      inputs: [lane('c'), lane('c', 1)],
      outputs: [],
    })
    expect(shape.paths).toEqual([
      { d: 'M 5.5 0 L 5.5 11', color: 0 },
      { d: 'M 16.5 0 C 16.5 5.5 5.5 5.5 5.5 11', color: 1 },
    ])
  })
})

describe('passingShape', () => {
  it('carries lanes straight through', () => {
    expect(passingShape([lane('a'), lane('b', 3)])?.paths).toEqual([
      { d: 'M 5.5 0 L 5.5 22', color: 0 },
      { d: 'M 16.5 0 L 16.5 22', color: 3 },
    ])
    expect(passingShape([])).toBeNull()
  })
})
