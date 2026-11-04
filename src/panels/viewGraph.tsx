/**
 * History graphs beside view items, in VS Code's style: each row's lanes
 * coming in at the top and leaving at the bottom, and its node. A lane for
 * the row's node ends at it; one for something else carries on to the
 * bottom lane for the same; and a lane leaves the node for each parent.
 */
import type { GraphLane, GraphRow } from '../ipc/extensions'

/** A lane's width and a row's height, in pixels. */
export const LANE_WIDTH = 11
export const ROW_HEIGHT = 22
const NODE_RADIUS = 4

const x = (lane: number) => lane * LANE_WIDTH + LANE_WIDTH / 2
const MID = ROW_HEIGHT / 2

export interface GraphPath {
  d: string
  color: number
}

export interface GraphShape {
  paths: GraphPath[]
  node: { x: number; color: number; merge: boolean }
  /** Lanes across, for the width. */
  lanes: number
}

/** A curve from one lane at `y0` to another at `y1`. */
function curve(from: number, y0: number, to: number, y1: number): string {
  if (from === to) {
    return `M ${x(from)} ${y0} L ${x(to)} ${y1}`
  }
  const middle = (y0 + y1) / 2
  return `M ${x(from)} ${y0} C ${x(from)} ${middle} ${x(to)} ${middle} ${x(to)} ${y1}`
}

/** What a graph row draws. */
export function graphShape(row: GraphRow): GraphShape {
  const found = row.inputs.findIndex(lane => lane.id === row.node)
  const node = found >= 0 ? found : row.inputs.length
  const paths: GraphPath[] = []
  row.inputs.forEach((lane, index) => {
    if (lane.id === row.node) {
      paths.push({ d: curve(index, 0, node, MID), color: lane.color })
      return
    }
    const to = row.outputs.findIndex(output => output.id === lane.id)
    paths.push({ d: curve(index, 0, to >= 0 ? to : index, ROW_HEIGHT), color: lane.color })
  })
  for (const parent of row.parents) {
    const to = row.outputs.findIndex(output => output.id === parent)
    if (to >= 0) {
      paths.push({ d: curve(node, MID, to, ROW_HEIGHT), color: row.outputs[to]!.color })
    }
  }
  const color =
    row.inputs[node]?.color ?? row.outputs.find(output => output.id === row.parents[0])?.color ?? 0
  return {
    paths,
    node: { x: x(node), color, merge: row.parents.length > 1 },
    lanes: Math.max(row.inputs.length, row.outputs.length, node + 1),
  }
}

/** Lanes passing straight through a row with no node (an opened commit's file). */
export function passingShape(lanes: readonly GraphLane[]): GraphShape | null {
  return lanes.length === 0
    ? null
    : {
        paths: lanes.map((lane, index) => ({
          d: curve(index, 0, index, ROW_HEIGHT),
          color: lane.color,
        })),
        node: { x: -1, color: 0, merge: false },
        lanes: lanes.length,
      }
}

/** One row's part of the graph. */
export function GraphCell({ shape, width }: { shape: GraphShape; width: number }) {
  return (
    <svg
      className="extension-view__graph"
      width={width}
      height={ROW_HEIGHT}
      viewBox={`0 0 ${width} ${ROW_HEIGHT}`}
      aria-hidden="true"
    >
      {shape.paths.map((path, index) => (
        <path key={index} d={path.d} data-lane={path.color} />
      ))}
      {shape.node.x >= 0 && (
        <circle
          cx={shape.node.x}
          cy={MID}
          r={NODE_RADIUS}
          data-lane={shape.node.color}
          data-merge={shape.node.merge || undefined}
        />
      )}
    </svg>
  )
}
