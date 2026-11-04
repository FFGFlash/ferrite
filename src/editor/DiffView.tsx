/**
 * A diff tab's content: two documents compared side by side (the original
 * on the left, read-only when an extension provides it) or inline (the
 * modified document with removed lines between its lines).
 *
 * Each side is an `EditorView` of its own view of the document (see
 * `state/diffs.ts`), so a file shown here can also be edited here. The diff
 * is computed again as either side changes. Lines line up across the sides
 * with filler, unchanged lines away from changes collapse (folded in both
 * views), and the sides scroll together. See `diffLayout.ts`.
 */
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import * as ipc from '../ipc/editor'
import type { DocumentSummary, EditorView as View, LineChange, PeekLines } from '../ipc/editor'
import { layoutStore } from '../state/layout'
import { reportError } from '../state/notifications'
import { diffViews, reloadsStore, setDiffControls, setDiffDirty } from '../state/diffs'
import { useStore } from '../state/store'
import { editorsFor, setActiveOverride, type EditorController } from './activeEditor'
import {
  changeAtLine,
  collapsedRegions,
  foldsFor,
  inlineZones,
  nextChange,
  sideBySideZones,
  type Collapsed,
  type DiffZone,
  type Side,
} from './diffLayout'
import { EditorView, type MarkedRange, type ViewZone } from './EditorView'
import { highlighted } from './highlighted'
import type { TextOverlay } from './segments'
import './DiffView.css'

/** Narrower than this, a diff shows inline whatever the setting, as in VS Code. */
export const INLINE_BELOW = 900
/** How long after an edit the diff is computed again. */
const DIFF_DELAY = 150
/** Most changes whose removed lines an inline diff fetches. */
const MAX_INLINE_CHANGES = 500

interface DiffViewProps {
  item: string
  original: DocumentSummary
  modified: DocumentSummary
}

export function DiffView({ item, original, modified }: DiffViewProps) {
  const settings = useStore(layoutStore).diff
  const views = diffViews(item)
  const frameRef = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(Number.POSITIVE_INFINITY)
  const inline = settings.inline || width < INLINE_BELOW

  useEffect(() => {
    const frame = frameRef.current
    if (!frame) {
      return
    }
    const observer = new ResizeObserver(() => setWidth(frame.clientWidth))
    observer.observe(frame)
    return () => observer.disconnect()
  }, [])

  // The sides' versions and line counts, from their views.
  const [state, setState] = useState({ originalVersion: -1, modifiedVersion: -1, modifiedLines: 0 })
  const cursorLineRef = useRef(0)
  const onOriginalView = (view: View) =>
    setState(current =>
      current.originalVersion === view.version
        ? current
        : { ...current, originalVersion: view.version },
    )
  const onModifiedView = (view: View) => {
    cursorLineRef.current = view.primary.position.line
    setDiffDirty(item, view.dirty)
    setState(current =>
      current.modifiedVersion === view.version && current.modifiedLines === view.lineCount
        ? current
        : { ...current, modifiedVersion: view.version, modifiedLines: view.lineCount },
    )
  }

  // The changes, computed again once the sides settle. Inline, the
  // original's editor isn't there to say it changed; its reloads do.
  const [changes, setChanges] = useState<LineChange[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const { originalVersion, modifiedVersion } = state
  const originalReloads = useStore(reloadsStore)[original.id] ?? 0
  const ignoreTrimWhitespace = settings.ignoreTrimWhitespace
  useEffect(() => {
    if ((!inline && originalVersion < 0) || modifiedVersion < 0) {
      return
    }
    let cancelled = false
    const timer = window.setTimeout(() => {
      ipc
        .computeDiff(original.id, modified.id, ignoreTrimWhitespace)
        .then(result => {
          if (!cancelled) {
            setChanges(result)
            setError(null)
          }
        })
        .catch((failure: unknown) => {
          if (!cancelled) {
            setError(failure instanceof Error ? failure.message : String(failure))
          }
        })
    }, DIFF_DELAY)
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [
    original.id,
    modified.id,
    inline,
    originalVersion,
    originalReloads,
    modifiedVersion,
    ignoreTrimWhitespace,
  ])

  // Unchanged stretches the user opened stay open.
  const [revealed, setRevealed] = useState<ReadonlySet<number>>(new Set())
  const lineCount = state.modifiedLines
  const collapsed = useMemo(
    () => (settings.hideUnchanged && changes ? collapsedRegions(changes, lineCount, revealed) : []),
    [settings.hideUnchanged, changes, lineCount, revealed],
  )
  const reveal = (region: Collapsed) => setRevealed(current => new Set(current).add(region.key))

  const editor = (side: Side): EditorController | undefined => {
    const document = side === 'original' ? original : modified
    return editorsFor(document.id).find(candidate => candidate.view === views[side])
  }

  // The collapsed lines are folded in each side's view.
  useEffect(() => {
    if (!changes) {
      return
    }
    for (const side of inline ? (['modified'] as const) : (['original', 'modified'] as const)) {
      editor(side)?.dispatch({ type: 'setFolds', regions: foldsFor(collapsed, side) }, 'none')
    }
    // `editor` reads the mounted editors, which follow `inline`.
  }, [collapsed, changes, inline])

  // In an inline diff, the removed lines, by change.
  const [removed, setRemoved] = useState<ReadonlyMap<number, PeekLines>>(new Map())
  useEffect(() => {
    if (!inline || !changes) {
      return
    }
    let cancelled = false
    const wanted = changes
      .map((change, index) => ({ change, index }))
      .filter(({ change }) => change.original[1] > change.original[0])
      .slice(0, MAX_INLINE_CHANGES)
    Promise.all(
      wanted.map(({ change, index }) =>
        ipc
          .documentLines(original.id, change.original[0], change.original[1])
          .then(lines => [index, lines] as const),
      ),
    )
      .then(entries => {
        if (!cancelled) {
          setRemoved(new Map(entries))
        }
      })
      .catch((failure: unknown) => reportError('Showing removed lines', failure))
    return () => {
      cancelled = true
    }
  }, [inline, changes, original.id])

  // Moving between changes (F7), for the commands.
  useEffect(() => {
    setDiffControls(item, {
      goToChange: backward => {
        const target = changes && nextChange(changes, cursorLineRef.current, backward)
        const shown = editor('modified')
        if (target && shown) {
          shown.dispatch(
            {
              type: 'setCursor',
              position: { line: target.modified[0], column: 0 },
              extend: false,
              add: false,
            },
            'center',
          )
          shown.focus()
        }
      },
    })
    return () => setDiffControls(item, null)
  })

  const zones = useMemo(() => {
    if (!changes) {
      return { original: NO_ZONES, modified: NO_ZONES }
    }
    const toViewZones = (list: DiffZone[], side: Side): ViewZone[] =>
      list.map((zone, index) => toViewZone(zone, index, side, changes, removed, reveal, lineCount))
    if (inline) {
      return {
        original: NO_ZONES,
        modified: toViewZones(inlineZones(changes, collapsed), 'modified'),
      }
    }
    const both = sideBySideZones(changes, collapsed)
    return {
      original: toViewZones(both.original, 'original'),
      modified: toViewZones(both.modified, 'modified'),
    }
  }, [changes, collapsed, inline, removed, lineCount])

  const lineClass = useMemo(() => {
    const list = changes ?? []
    return {
      original: (line: number) =>
        changeAtLine(list, 'original', line) ? 'diff-removed' : undefined,
      modified: (line: number) =>
        changeAtLine(list, 'modified', line) ? 'diff-inserted' : undefined,
    }
  }, [changes])

  const marks = useMemo(() => {
    const result: Record<Side, MarkedRange[]> = { original: [], modified: [] }
    for (const change of changes ?? []) {
      for (const chars of change.chars) {
        result.original.push({ ...chars.original, className: 'diff-removed-text' })
        result.modified.push({ ...chars.modified, className: 'diff-inserted-text' })
      }
    }
    return result
  }, [changes])

  // The sides scroll together.
  const syncing = useRef(false)
  const follow = (to: Side) => (scroller: HTMLElement) => {
    if (inline || syncing.current) {
      return
    }
    syncing.current = true
    editor(to)?.restoreScroll({ top: scroller.scrollTop, left: scroller.scrollLeft })
    syncing.current = false
  }

  // Commands act on the side in use.
  const onFocus = (side: Side) => () => setActiveOverride(editor(side) ?? null)

  return (
    <div className="diff-view" ref={frameRef} data-inline={inline || undefined}>
      {error && (
        <div className="diff-view__message" role="status">
          {error}
        </div>
      )}
      <div className="diff-view__sides">
        {!inline && (
          <div className="diff-view__side" data-side="original" onFocus={onFocus('original')}>
            <EditorView
              documentId={original.id}
              view={views.original}
              title={`${original.title} (original)`}
              zones={zones.original}
              lineClass={lineClass.original}
              marks={marks.original}
              folding={false}
              minimap={false}
              quickDiff={false}
              onView={onOriginalView}
              onScroll={follow('modified')}
            />
          </div>
        )}
        <div className="diff-view__side" data-side="modified" onFocus={onFocus('modified')}>
          <EditorView
            documentId={modified.id}
            view={views.modified}
            title={modified.title}
            zones={zones.modified}
            lineClass={lineClass.modified}
            marks={marks.modified}
            folding={false}
            minimap={false}
            quickDiff={false}
            onView={onModifiedView}
            onScroll={follow('original')}
          />
          <OverviewRuler changes={changes ?? []} lineCount={state.modifiedLines} inline={inline} />
        </div>
      </div>
    </div>
  )
}

const NO_ZONES: readonly ViewZone[] = []

function toViewZone(
  zone: DiffZone,
  index: number,
  side: Side,
  changes: readonly LineChange[],
  removed: ReadonlyMap<number, PeekLines>,
  reveal: (region: Collapsed) => void,
  lineCount: number,
): ViewZone {
  const key = `${zone.kind}:${zone.row}:${index}`
  switch (zone.kind) {
    case 'filler':
      return { key, row: zone.row, lines: zone.lines, className: 'diff-filler' }
    case 'collapsed': {
      const count = zone.region[side][1] - zone.region[side][0]
      return {
        key,
        row: zone.row,
        lines: 1,
        className: 'diff-collapsed',
        content: (
          <button
            type="button"
            className="diff-view__collapsed"
            title="Show the unchanged lines"
            onMouseDown={event => event.stopPropagation()}
            onClick={() => reveal(zone.region)}
          >
            ⋯ {count.toLocaleString()} unchanged {count === 1 ? 'line' : 'lines'}
          </button>
        ),
      }
    }
    case 'removed':
      return {
        key,
        row: zone.row,
        lines: zone.lines,
        className: 'diff-removed-zone',
        content: (
          <RemovedLines
            change={changes[zone.change]!}
            lines={removed.get(zone.change) ?? null}
            numberWidth={String(Math.max(lineCount, 1)).length}
          />
        ),
      }
  }
}

/** Removed lines in an inline diff, with their original line numbers. */
export function RemovedLines({
  change,
  lines,
  numberWidth,
}: {
  change: LineChange
  lines: PeekLines | null
  numberWidth: number
}): ReactNode {
  const count = change.original[1] - change.original[0]
  return (
    <div className="diff-view__removed">
      {Array.from({ length: count }, (_, offset) => {
        const line = change.original[0] + offset
        const shown = lines?.lines[line - lines.start]
        const overlays: TextOverlay[] = shown
          ? change.chars.flatMap(({ original }) =>
              original.start.line <= line && line <= original.end.line
                ? [
                    {
                      from: original.start.line === line ? original.start.column : 0,
                      to: original.end.line === line ? original.end.column : shown.text.length,
                      className: 'diff-removed-text',
                    },
                  ]
                : [],
            )
          : []
        return (
          <div key={line} className="diff-view__removed-line">
            <span
              className="diff-view__removed-number"
              style={{ width: `calc(${numberWidth}ch + 32px + 20px)` }}
            >
              {line + 1}
            </span>
            <span className="diff-view__removed-text">
              {shown ? highlighted(shown.text, shown.spans, overlays) : ''}
            </span>
          </div>
        )
      })}
    </div>
  )
}

/** Marks beside the modified side where its changes are, over the whole document. */
function OverviewRuler({
  changes,
  lineCount,
  inline,
}: {
  changes: readonly LineChange[]
  lineCount: number
  inline: boolean
}) {
  if (lineCount === 0) {
    return null
  }
  return (
    <div className="diff-view__ruler" aria-hidden="true">
      {changes.map((change, index) => {
        const inserted = change.modified[1] - change.modified[0]
        const removedOnly = inserted === 0
        const top = (change.modified[0] / lineCount) * 100
        const height = (Math.max(inserted, 1) / lineCount) * 100
        return (
          <div
            key={index}
            className="diff-view__ruler-mark"
            data-kind={
              removedOnly
                ? 'removed'
                : change.original[1] > change.original[0] && !inline
                  ? 'changed'
                  : 'inserted'
            }
            style={{ top: `${top}%`, height: `max(${height}%, 2px)` }}
          />
        )
      })}
    </div>
  )
}
