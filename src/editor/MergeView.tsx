/**
 * A merge tab's content, as VS Code's merge editor: the incoming version
 * (left) and the current one (right) above the result, the file being
 * merged, which can be edited freely.
 *
 * The backend compares each version with their base and the result with
 * the base (`merge_compute`), giving each range a side changed and what
 * the result has of it. Above each range, each editor has a row of
 * buttons: the versions' accept their change, the result's accept either
 * side, both, or neither. The editors line up (see `mergeLayout.ts`) and
 * scroll together. Completing the merge saves the result and tells the
 * extension that opened it.
 */
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { ArrowDown, ArrowUp } from 'lucide-react'
import * as ipc from '../ipc/editor'
import type { EditorView as View, MergeInfo, MergeRange, MergeRangeState } from '../ipc/editor'
import type { MergeSpec } from '../ipc/extensions'
import { ModalDialog } from '../components/ModalDialog'
import { reportError } from '../state/notifications'
import {
  completeMerge,
  mergeDocuments,
  mergeViews,
  setMergeControls,
  setMergeDirty,
  type MergeState,
} from '../state/merges'
import { editorsFor, setActiveOverride, type EditorController } from './activeEditor'
import { EditorView, type ViewZone } from './EditorView'
import {
  lineClassOf,
  mergeBlocks,
  mergeZones,
  nextConflict,
  stateLabel,
  toggled,
  unresolved,
  type MergeZone,
  type Pane,
} from './mergeLayout'
import './MergeView.css'

/** How long after an edit the merge is computed again. */
const MERGE_DELAY = 150

interface MergeViewProps {
  item: string
  spec: MergeSpec
  state: Extract<MergeState, { status: 'ready' }>
}

export function MergeView({ item, spec, state }: MergeViewProps) {
  const views = mergeViews(item)
  const documents = useMemo(() => mergeDocuments(state), [state])
  const document = (pane: Pane) => state[pane]

  // The result's version and the versions', from their views.
  const [versions, setVersions] = useState({ incoming: -1, current: -1, result: -1 })
  const cursorLineRef = useRef(0)
  const onView = (pane: Pane) => (view: View) => {
    if (pane === 'result') {
      cursorLineRef.current = view.primary.position.line
      setMergeDirty(item, view.dirty)
    }
    setVersions(current =>
      current[pane] === view.version ? current : { ...current, [pane]: view.version },
    )
  }

  const [info, setInfo] = useState<MergeInfo | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    if (versions.result < 0) {
      return
    }
    let cancelled = false
    const timer = window.setTimeout(() => {
      ipc
        .computeMerge(documents)
        .then(result => {
          if (!cancelled) {
            setInfo(result)
            setError(null)
          }
        })
        .catch((failure: unknown) => {
          if (!cancelled) {
            setError(failure instanceof Error ? failure.message : String(failure))
          }
        })
    }, MERGE_DELAY)
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [documents, versions.incoming, versions.current, versions.result])

  const ranges = info?.ranges ?? NO_RANGES
  const remaining = ranges.filter(unresolved).length
  const conflicts = ranges.filter(range => range.conflicting).length

  const editor = (pane: Pane): EditorController | undefined =>
    editorsFor(document(pane).id).find(candidate => candidate.view === views[pane])

  const set = (index: number, to: MergeRangeState) => {
    if (!info) {
      return
    }
    ipc
      .setMergeRange(documents, info.version, index, to)
      .catch((failure: unknown) => reportError('Accepting the change', failure))
  }

  const goTo = (range: MergeRange) => {
    const shown = editor('result')
    shown?.dispatch(
      {
        type: 'setCursor',
        position: { line: range.result[0], column: 0 },
        extend: false,
        add: false,
      },
      'center',
    )
    shown?.focus()
  }

  useEffect(() => {
    setMergeControls(item, {
      goToConflict: backward => {
        const target = nextConflict(ranges, cursorLineRef.current, backward)
        if (target) {
          goTo(target)
        }
      },
    })
    return () => setMergeControls(item, null)
  })

  const zones = useMemo(() => {
    const byPane = info ? mergeZones(mergeBlocks(info)) : null
    const toViewZones = (pane: Pane): readonly ViewZone[] =>
      byPane?.[pane].map((zone, index) => toViewZone(zone, index, pane, ranges, set)) ?? NO_ZONES
    return {
      incoming: toViewZones('incoming'),
      current: toViewZones('current'),
      result: toViewZones('result'),
    }
    // `set` reads the latest info, which the memo follows.
  }, [info, ranges])

  const lineClass = useMemo(
    () => ({
      incoming: (line: number) => lineClassOf(ranges, 'incoming', line),
      current: (line: number) => lineClassOf(ranges, 'current', line),
      result: (line: number) => lineClassOf(ranges, 'result', line),
    }),
    [ranges],
  )

  // The editors scroll together.
  const syncing = useRef(false)
  const follow = (from: Pane) => (scroller: HTMLElement) => {
    if (syncing.current) {
      return
    }
    syncing.current = true
    for (const pane of ['incoming', 'current', 'result'] as const) {
      if (pane !== from) {
        editor(pane)?.restoreScroll({ top: scroller.scrollTop, left: scroller.scrollLeft })
      }
    }
    syncing.current = false
  }

  const onFocus = (pane: Pane) => () => setActiveOverride(editor(pane) ?? null)

  const [confirming, setConfirming] = useState(false)
  const complete = () => {
    if (remaining > 0) {
      setConfirming(true)
    } else {
      void completeMerge(item)
    }
  }

  const pane = (which: Pane, title: string, description: string | null, actions?: ReactNode) => (
    <div className="merge-view__pane" data-pane={which} onFocus={onFocus(which)}>
      <div className="merge-view__header">
        <span className="merge-view__title">{title}</span>
        {description && <span className="merge-view__description">{description}</span>}
        {actions}
      </div>
      <div className="merge-view__editor">
        <EditorView
          documentId={document(which).id}
          view={views[which]}
          title={`${state[which].title} (${title})`}
          zones={zones[which]}
          lineClass={lineClass[which]}
          folding={false}
          minimap={false}
          quickDiff={false}
          onView={onView(which)}
          onScroll={follow(which)}
        />
      </div>
    </div>
  )

  return (
    <div className="merge-view">
      {error && (
        <div className="merge-view__message" role="status">
          {error}
        </div>
      )}
      <div className="merge-view__inputs">
        {pane('incoming', spec.incoming.title, spec.incoming.description)}
        {pane('current', spec.current.title, spec.current.description)}
      </div>
      {pane(
        'result',
        'Result',
        spec.result,
        <span className="merge-view__actions">
          <span className="merge-view__remaining" role="status">
            {!info
              ? ''
              : conflicts === 0
                ? 'No conflicts'
                : remaining === 0
                  ? 'All conflicts resolved'
                  : `${remaining} of ${conflicts} ${conflicts === 1 ? 'conflict' : 'conflicts'} remaining`}
          </span>
          <button
            type="button"
            className="merge-view__icon-button"
            title="Previous Conflict"
            aria-label="Previous Conflict"
            disabled={conflicts === 0}
            onClick={() => {
              const target = nextConflict(ranges, cursorLineRef.current, true)
              if (target) {
                goTo(target)
              }
            }}
          >
            <ArrowUp size={16} aria-hidden="true" />
          </button>
          <button
            type="button"
            className="merge-view__icon-button"
            title="Next Conflict"
            aria-label="Next Conflict"
            disabled={conflicts === 0}
            onClick={() => {
              const target = nextConflict(ranges, cursorLineRef.current, false)
              if (target) {
                goTo(target)
              }
            }}
          >
            <ArrowDown size={16} aria-hidden="true" />
          </button>
          <button type="button" className="merge-view__complete" onClick={complete}>
            Complete Merge
          </button>
        </span>,
      )}
      {confirming && (
        <ModalDialog
          title={`${remaining} ${remaining === 1 ? 'conflict is' : 'conflicts are'} not resolved yet.`}
          onClose={() => setConfirming(false)}
          buttons={
            <>
              <button type="button" className="dialog__button" onClick={() => setConfirming(false)}>
                Cancel
              </button>
              <button
                type="button"
                className="dialog__button dialog__button--primary"
                onClick={() => {
                  setConfirming(false)
                  void completeMerge(item)
                }}
              >
                Complete Anyway
              </button>
            </>
          }
        >
          The result keeps the base's lines where a conflict is unresolved.
        </ModalDialog>
      )}
    </div>
  )
}

const NO_ZONES: readonly ViewZone[] = []
const NO_RANGES: readonly MergeRange[] = []

function toViewZone(
  zone: MergeZone,
  index: number,
  pane: Pane,
  ranges: readonly MergeRange[],
  set: (index: number, to: MergeRangeState) => void,
): ViewZone {
  const key = `${zone.kind}:${zone.row}:${index}`
  if (zone.kind === 'filler') {
    return { key, row: zone.row, lines: zone.lines, className: 'diff-filler' }
  }
  const range = ranges[zone.range]!
  return {
    key,
    row: zone.row,
    lines: 1,
    className: 'merge-actions',
    content: <RangeActions range={range} pane={pane} set={to => set(zone.range, to)} />,
  }
}

/** The buttons above a range in one of the editors. */
function RangeActions({
  range,
  pane,
  set,
}: {
  range: MergeRange
  pane: Pane
  set: (to: MergeRangeState) => void
}) {
  const links: { label: string; to: MergeRangeState }[] = []
  let label: string | null = null
  if (pane === 'result') {
    label = stateLabel(range)
    if (range.conflicting) {
      links.push(
        { label: 'Accept Incoming', to: 'incoming' },
        { label: 'Accept Current', to: 'current' },
        { label: 'Accept Both', to: 'currentIncoming' },
      )
    }
    if (range.state !== 'base') {
      links.push({ label: 'Reset', to: 'base' })
    }
  } else {
    const changed = pane === 'incoming' ? range.incomingChanged : range.currentChanged
    if (!changed) {
      return null
    }
    const name = pane === 'incoming' ? 'Incoming' : 'Current'
    const to = toggled(range, pane)
    const accepted =
      range.state === pane || range.state === 'currentIncoming' || range.state === 'incomingCurrent'
    if (accepted) {
      label = '✓ Accepted'
      links.push({ label: 'Undo', to })
    } else {
      links.push({ label: `Accept ${name}`, to: range.conflicting ? pane : to })
      if (range.conflicting) {
        links.push({
          label: 'Accept Combination',
          to: pane === 'incoming' ? 'incomingCurrent' : 'currentIncoming',
        })
      }
    }
  }
  return (
    <div
      className="merge-view__range-actions"
      data-conflict={range.conflicting || undefined}
      onMouseDown={event => event.stopPropagation()}
    >
      {label && <span className="merge-view__range-label">{label}</span>}
      {links.map((link, index) => (
        <span key={link.label}>
          {(index > 0 || label) && (
            <span className="merge-view__separator" aria-hidden="true">
              |
            </span>
          )}
          <button type="button" className="merge-view__link" onClick={() => set(link.to)}>
            {link.label}
          </button>
        </span>
      ))}
    </div>
  )
}
