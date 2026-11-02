import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent,
  type RefObject,
} from 'react'
import { ChevronDown, ChevronRight, X } from 'lucide-react'
import { FileIcon } from '../components/FileIcon'
import { entryIcon } from '../components/fileIcons'
import { Sash } from '../components/Sash'
import { peekLines, type PeekLines, type Position } from '../ipc/editor'
import type { Reference } from '../ipc/lsp'
import { iconThemeStore } from '../state/iconTheme'
import { useStore } from '../state/store'
import { workspaceStore } from '../state/workspace'
import { themeKind } from '../themes/theme'
import { highlighted } from './highlighted'
import { columnAt, columnX } from './measure'
import {
  closePeek,
  describePath,
  openReference,
  peekLayout,
  referenceAt,
  referenceCount,
  referencePreview,
  selectReference,
  toggleGroup,
  type PeekState,
  type ReferenceIndex,
} from './references'
import type { TextOverlay } from './segments'
import './PeekView.css'

/** Narrowest the preview or the list gets, in pixels. */
const MIN_PANE_WIDTH = 120
/** Fewest lines the peek can be resized to. */
const MIN_HEIGHT_IN_LINES = 5
/** Lines fetched for the preview on each side of what it shows. */
const PREVIEW_WINDOW = 150

interface PeekViewProps {
  state: PeekState
  lineHeight: number
  /** Its size in pixels: the editor's visible width, and the zone's height. */
  width: number
  height: number
  /** Where its arrow points, from its left edge; `null` to leave it out. */
  arrowLeft: number | null
}

/**
 * A peek at references inside the editor (see `references.ts`): a header
 * naming the selected reference's file, a read-only preview of that file,
 * and the list of references by file.
 */
export function PeekView({ state, lineHeight, width, height, arrowLeft }: PeekViewProps) {
  const { info } = useStore(workspaceStore)
  const { ratio } = useStore(peekLayout)
  const listRef = useRef<HTMLDivElement>(null)
  const previewRef = useRef<HTMLDivElement>(null)
  const reference = referenceAt(state)
  const group = state.groups[state.selected.group]!
  const { name, folder } = describePath(group.path, info?.root ?? null)

  useLayoutEffect(() => {
    const target = state.focus === 'list' ? listRef : state.focus === 'preview' ? previewRef : null
    target?.current?.focus({ preventScroll: true })
    // Only when asked: the rest of the state changes as the list is used.
  }, [state.focusRequest])

  const arrowSize = Math.round(lineHeight / 3)
  const headHeight = Math.ceil(lineHeight * 1.2)
  // Two pixels of border, above and below.
  const bodyHeight = Math.max(0, height - arrowSize - headHeight - 2)
  const previewWidth = Math.min(
    Math.max(MIN_PANE_WIDTH, Math.round(width * ratio)),
    Math.max(MIN_PANE_WIDTH, width - MIN_PANE_WIDTH),
  )

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Escape' && !event.ctrlKey && !event.altKey && !event.metaKey) {
      event.preventDefault()
      event.stopPropagation()
      closePeek()
    }
  }

  return (
    <div
      className="peek"
      role="region"
      aria-label={`${state.title}: ${name}`}
      style={{ height }}
      onKeyDown={onKeyDown}
    >
      <div className="peek__arrow-row" style={{ height: arrowSize }}>
        {arrowLeft !== null && arrowLeft >= 0 && arrowLeft < width && (
          <div
            className="peek__arrow"
            style={{
              left: arrowLeft,
              marginLeft: -arrowSize,
              borderWidth: `0 ${arrowSize}px ${arrowSize}px`,
            }}
          />
        )}
      </div>
      <div className="peek__frame">
        <div className="peek__head" style={{ height: headHeight }}>
          <button
            type="button"
            className="peek__title"
            title={group.path}
            onClick={() => void openReference(reference)}
          >
            <span className="peek__name">{name}</span>
            {folder && <span className="peek__folder">{folder}</span>}
          </button>
          <span className="peek__meta">
            {state.title} ({referenceCount(state.groups)})
          </span>
          <button
            type="button"
            className="peek__close"
            aria-label="Close"
            title="Close (Escape)"
            onClick={() => closePeek()}
          >
            <X size={16} strokeWidth={1.75} aria-hidden="true" />
          </button>
        </div>
        <div className="peek__body" style={{ height: bodyHeight }}>
          <div className="peek__preview-pane" style={{ width: previewWidth }}>
            <PeekPreview
              path={group.path}
              name={name}
              references={group.references}
              selected={reference}
              lineHeight={lineHeight}
              scrollRef={previewRef}
              onOpen={at => void openReference(reference, at)}
            />
            <Sash
              label="Resize the preview"
              value={previewWidth}
              min={MIN_PANE_WIDTH}
              max={Math.max(MIN_PANE_WIDTH, width - MIN_PANE_WIDTH)}
              onChange={next =>
                peekLayout.update(layout => ({ ...layout, ratio: next / Math.max(1, width) }))
              }
            />
          </div>
          <PeekList state={state} listRef={listRef} />
        </div>
      </div>
      <ResizeHandle height={height} lineHeight={lineHeight} />
    </div>
  )
}

/** The peek's bottom edge: dragging it makes the peek taller or shorter, in whole lines. */
function ResizeHandle({ height, lineHeight }: { height: number; lineHeight: number }) {
  const drag = useRef<{ y: number; height: number } | null>(null)
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current) {
      const lines = Math.round((drag.current.height + event.clientY - drag.current.y) / lineHeight)
      peekLayout.update(layout => ({
        ...layout,
        heightInLines: Math.max(MIN_HEIGHT_IN_LINES, lines),
      }))
    }
  }
  return (
    <div
      className="peek__resize"
      aria-hidden="true"
      onPointerDown={event => {
        if (event.button === 0) {
          event.preventDefault()
          event.currentTarget.setPointerCapture(event.pointerId)
          drag.current = { y: event.clientY, height }
        }
      }}
      onPointerMove={onPointerMove}
      onPointerUp={() => (drag.current = null)}
      onPointerCancel={() => (drag.current = null)}
    />
  )
}

interface PreviewProps {
  path: string
  name: string
  /** The references in this file. */
  references: readonly Reference[]
  selected: Reference
  lineHeight: number
  scrollRef: RefObject<HTMLDivElement | null>
  /** Opens the file at a position double-clicked. */
  onOpen: (at: Position) => void
}

/**
 * The selected reference's file, read only, scrolled to it: its references
 * marked, the selected one as selected. Lines are fetched in windows around
 * what's shown.
 */
function PeekPreview({
  path,
  name,
  references,
  selected,
  lineHeight,
  scrollRef,
  onOpen,
}: PreviewProps) {
  const linesRef = useRef<HTMLDivElement>(null)
  const gutterRef = useRef<HTMLDivElement>(null)
  const [data, setData] = useState<(PeekLines & { path: string }) | null>(null)
  const requestRef = useRef(0)
  /** Set when the selected reference should be scrolled to once its line arrives. */
  const revealRef = useRef(true)

  const fetchAround = (line: number) => {
    const seq = ++requestRef.current
    peekLines(path, Math.max(0, line - PREVIEW_WINDOW), line + PREVIEW_WINDOW).then(
      lines => {
        if (seq === requestRef.current) {
          setData({ ...lines, path })
        }
      },
      error => console.debug('peek preview failed', error),
    )
  }

  useEffect(() => {
    revealRef.current = true
    fetchAround(selected.start.line)
  }, [path, selected])

  const shown = data?.path === path ? data : null

  useLayoutEffect(() => {
    const scroller = scrollRef.current
    const line = selected.start.line
    if (!revealRef.current || !shown || !scroller) {
      return
    }
    const element = linesRef.current?.children[line - shown.start]
    if (!element) {
      return
    }
    revealRef.current = false
    scroller.scrollTop = line * lineHeight - (scroller.clientHeight - lineHeight) / 2
    const textWidth = scroller.clientWidth - (gutterRef.current?.offsetWidth ?? 0)
    const x = columnX(element, selected.start.column)
    scroller.scrollLeft = x > textWidth - 2 * lineHeight ? x - textWidth / 2 : 0
  })

  const onScroll = () => {
    const scroller = scrollRef.current
    if (!scroller || !shown) {
      return
    }
    const first = Math.floor(scroller.scrollTop / lineHeight)
    const last = Math.ceil((scroller.scrollTop + scroller.clientHeight) / lineHeight)
    const end = shown.start + shown.lines.length
    if ((first < shown.start && shown.start > 0) || (last > end && end < shown.lineCount)) {
      fetchAround(Math.floor((first + last) / 2))
    }
  }

  const onDoubleClick = (event: ReactMouseEvent<HTMLDivElement>) => {
    const lines = linesRef.current
    if (!lines || !shown) {
      return
    }
    const row = Math.floor((event.clientY - lines.getBoundingClientRect().top) / lineHeight)
    const element = lines.children[row]
    if (element) {
      onOpen({ line: shown.start + row, column: columnAt(element, event.clientX, event.clientY) })
    }
  }

  const lineCount = shown?.lineCount ?? 0
  const offset = { transform: `translateY(${(shown?.start ?? 0) * lineHeight}px)` }
  return (
    <div
      ref={scrollRef}
      className="peek-preview"
      tabIndex={0}
      aria-label={`Preview of ${name}`}
      onScroll={onScroll}
      onDoubleClick={onDoubleClick}
    >
      <div className="peek-preview__sizer" style={{ height: lineCount * lineHeight }}>
        <div
          ref={gutterRef}
          className="peek-preview__gutter"
          style={{ width: `calc(${String(lineCount).length}ch + 24px)` }}
          aria-hidden="true"
        >
          <div style={offset}>
            {shown?.lines.map((_, index) => (
              <div key={shown.start + index} className="peek-preview__number">
                {shown.start + index + 1}
              </div>
            ))}
          </div>
        </div>
        <div ref={linesRef} className="peek-preview__lines" style={offset}>
          {shown?.lines.map((line, index) => {
            const number = shown.start + index
            const overlays = matchesOn(number, line.text.length, references, selected)
            return (
              <div key={number} className="peek-preview__line">
                {line.spans.length === 0 && overlays.length === 0
                  ? line.text
                  : highlighted(line.text, line.spans, overlays)}
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}

/** The parts of `line` (of `length`) that references cover, marked; the selected one as selected. */
function matchesOn(
  line: number,
  length: number,
  references: readonly Reference[],
  selected: Reference,
): TextOverlay[] {
  const overlays: TextOverlay[] = []
  for (const reference of references) {
    if (reference.start.line > line || reference.end.line < line) {
      continue
    }
    const current = reference === selected
    overlays.push({
      from: reference.start.line === line ? reference.start.column : 0,
      to: reference.end.line === line ? reference.end.column : length,
      className: current
        ? 'peek-preview__match peek-preview__match--selected'
        : 'peek-preview__match',
    })
  }
  return overlays
}

/** A row of the list: a file (`index` null) or one of its references. */
interface Row {
  group: number
  index: number | null
}

function rowsOf(state: PeekState): Row[] {
  // References in a single file are listed without it.
  if (state.groups.length === 1) {
    return state.groups[0]!.references.map((_, index) => ({ group: 0, index }))
  }
  return state.groups.flatMap((group, groupIndex) => [
    { group: groupIndex, index: null },
    ...(state.collapsed.has(group.path)
      ? []
      : group.references.map((_, index) => ({ group: groupIndex, index }))),
  ])
}

const sameRow = (a: Row, b: Row) => a.group === b.group && a.index === b.index

/** The references, by file: a tree navigated with the arrow keys. */
function PeekList({
  state,
  listRef,
}: {
  state: PeekState
  listRef: RefObject<HTMLDivElement | null>
}) {
  const id = useId()
  const iconTheme = useStore(iconThemeStore)
  const light = useStore(themeKind) === 'light'
  const { info } = useStore(workspaceStore)
  const rows = rowsOf(state)
  const multiple = state.groups.length > 1
  const [focused, setFocused] = useState<Row>(state.selected)
  // Moving to another reference (F4) moves the focus with it.
  useEffect(() => setFocused(state.selected), [state.selected])
  const focusedIndex = Math.max(
    0,
    rows.findIndex(row => sameRow(row, focused)),
  )
  const rowId = (row: Row) => `${id}-${row.group}-${row.index ?? 'file'}`

  useLayoutEffect(() => {
    const list = listRef.current
    const element = list?.querySelector<HTMLElement>(`[data-row="${focusedIndex}"]`)
    if (!list || !element) {
      return
    }
    if (element.offsetTop < list.scrollTop) {
      list.scrollTop = element.offsetTop
    } else if (element.offsetTop + element.offsetHeight > list.scrollTop + list.clientHeight) {
      list.scrollTop = element.offsetTop + element.offsetHeight - list.clientHeight
    }
  })

  /** Moves focus to a row, showing it in the preview if it's a reference. */
  const focusRow = (row: Row | undefined) => {
    if (!row) {
      return
    }
    setFocused(row)
    if (row.index !== null) {
      selectReference(row as ReferenceIndex)
    }
  }

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.altKey || event.metaKey) {
      return
    }
    const row = rows[focusedIndex]
    if (!row) {
      return
    }
    const path = state.groups[row.group]!.path
    const expanded = !state.collapsed.has(path)
    const page = Math.max(1, Math.floor((listRef.current?.clientHeight ?? 0) / 22) - 1)
    switch (event.key) {
      case 'ArrowDown':
        focusRow(rows[focusedIndex + 1])
        break
      case 'ArrowUp':
        focusRow(rows[focusedIndex - 1])
        break
      case 'PageDown':
        focusRow(rows[Math.min(rows.length - 1, focusedIndex + page)])
        break
      case 'PageUp':
        focusRow(rows[Math.max(0, focusedIndex - page)])
        break
      case 'Home':
        focusRow(rows[0])
        break
      case 'End':
        focusRow(rows.at(-1))
        break
      case 'ArrowRight':
        if (row.index === null) {
          if (expanded) {
            focusRow(rows[focusedIndex + 1])
          } else {
            toggleGroup(path)
          }
        }
        break
      case 'ArrowLeft':
        if (row.index === null) {
          if (expanded) {
            toggleGroup(path)
          }
        } else if (multiple) {
          setFocused({ group: row.group, index: null })
        }
        break
      case 'Enter':
        if (row.index === null) {
          toggleGroup(path)
        } else {
          void openReference(referenceAt(state, row as ReferenceIndex))
        }
        break
      case ' ':
        if (row.index === null) {
          toggleGroup(path)
        } else {
          selectReference(row as ReferenceIndex)
        }
        break
      default:
        return
    }
    event.preventDefault()
  }

  const focusedRow = rows[focusedIndex]
  return (
    <div
      ref={listRef}
      className="peek-list"
      role="tree"
      aria-label={state.title}
      aria-activedescendant={focusedRow ? rowId(focusedRow) : undefined}
      tabIndex={0}
      onKeyDown={onKeyDown}
    >
      {rows.map((row, rowIndex) => {
        const group = state.groups[row.group]!
        const common = {
          'id': rowId(row),
          'role': 'treeitem',
          'data-row': rowIndex,
          'data-focused': rowIndex === focusedIndex || undefined,
        }
        if (row.index === null) {
          const expanded = !state.collapsed.has(group.path)
          const { name, folder } = describePath(group.path, info?.root ?? null)
          const count = group.references.length
          return (
            <div
              key={group.path}
              {...common}
              className="peek-list__row peek-list__file"
              aria-level={1}
              aria-expanded={expanded}
              aria-selected={false}
              title={group.path}
              onMouseDown={() => {
                setFocused(row)
                toggleGroup(group.path)
              }}
            >
              <span className="peek-list__chevron" aria-hidden="true">
                {expanded ? (
                  <ChevronDown size={14} strokeWidth={1.75} />
                ) : (
                  <ChevronRight size={14} strokeWidth={1.75} />
                )}
              </span>
              <FileIcon
                icon={entryIcon(name, null, iconTheme, light)}
                kind="file"
                className="peek-list__icon"
              />
              <span className="peek-list__name">{name}</span>
              {folder && <span className="peek-list__folder">{folder}</span>}
              <span
                className="peek-list__count"
                title={`${count} reference${count === 1 ? '' : 's'}`}
              >
                {count}
              </span>
            </div>
          )
        }
        const reference = group.references[row.index]!
        const { before, inside, after } = referencePreview(reference)
        const selected = sameRow(row, state.selected)
        return (
          <div
            key={`${group.path}:${row.index}`}
            {...common}
            className="peek-list__row peek-list__reference"
            data-nested={multiple || undefined}
            aria-level={multiple ? 2 : 1}
            aria-selected={selected}
            title={`Line ${reference.start.line + 1}, column ${reference.start.column + 1}`}
            onMouseDown={() => focusRow(row)}
            onDoubleClick={() => void openReference(reference)}
          >
            {before}
            <mark className="peek-list__match">{inside}</mark>
            {after}
          </div>
        )
      })}
    </div>
  )
}
