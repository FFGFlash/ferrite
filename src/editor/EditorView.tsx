import {
  memo,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ClipboardEvent,
  type CompositionEvent,
  type FormEvent,
  type KeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from 'react'
import {
  type DisplayLine,
  type EditorOp,
  type EditorView as View,
  type Position,
  type SearchQuery,
  type Viewport,
} from '../ipc/editor'
import * as lsp from '../ipc/lsp'
import { appearance } from '../state/appearance'
import {
  clearPendingSelection,
  openPath,
  pendingSelection,
  registerBeforeSave,
  setDirty,
} from '../state/documents'
import { editorStatus } from '../state/editorStatus'
import { notify, reportError } from '../state/notifications'
import { notifyPainted, recordTiming } from '../state/perf'
import { useStore } from '../state/store'
import { activeEditor, type EditorController } from './activeEditor'
import { isWordChar, visibleCompletions, wordAround } from './completion'
import { FindWidget, type FindState } from './FindWidget'
import { inputMirror, typedText, type InputMirror } from './inputMirror'
import { isMac, keyToCommand } from './keymap'
import {
  COMPLETION_ROW_HEIGHT,
  COMPLETION_ROWS,
  COMPLETION_WIDTH,
  CompletionList,
  HoverCard,
  severityName,
  type Placement,
} from './LanguagePopups'
import {
  columnAt,
  columnX,
  EMPTY_OVERLAY,
  lineLength,
  measureOverlay,
  type Overlay,
  type Rect,
} from './measure'
import { openContextMenu, type MenuEntry } from '../components/contextMenuState'
import { openPalette } from '../components/paletteState'
import { getCommand, primaryKeybinding } from '../commands/registry'
import {
  contributedMenuItems,
  decorationTypes,
  extensionsStore,
  runContributed,
} from '../state/extensions'
import { themeKind } from '../themes/theme'
import { decorationRectStyle, textOverlays } from './decorations'
import { LINK_CLASS, segments, type TextOverlay } from './segments'
import { EditorSession, ipcTransport, type EditorTransport } from './session'
import './EditorView.css'

/** Extra lines fetched above and below the visible area. */
const OVERSCAN = 20
/** A column beyond any line end; the backend clamps it to the line end. */
const END_OF_LINE = Number.MAX_SAFE_INTEGER
const EMPTY_QUERY: SearchQuery = {
  pattern: '',
  regex: false,
  caseSensitive: false,
  wholeWord: false,
}

/** How long the pointer rests on a symbol before its hover card shows. */
const HOVER_DELAY = 500
/** How far the pointer may drift along a line while a hover card stays. */
const HOVER_SLACK = 40

interface HoverState {
  hover: lsp.Hover
  line: number
  x: number
  placement: Placement
}

interface CompletionState {
  items: lsp.CompletionItem[]
  incomplete: boolean
  selected: number
}

/** A word under the pointer, as UTF-16 columns `[from, to)` of `line`. */
interface Word {
  line: number
  from: number
  to: number
}

/** The symbol drawn as a link while the go-to-definition modifier is held. */
interface LinkState extends Word {
  /** The document version it was found in; edits hide it. */
  version: number
}

/** Whether `event` holds the go-to-definition modifier (Ctrl, or ⌘ on macOS) alone. */
function definitionModifier(event: {
  ctrlKey: boolean
  metaKey: boolean
  shiftKey: boolean
  altKey: boolean
}) {
  return (isMac() ? event.metaKey : event.ctrlKey) && !event.shiftKey && !event.altKey
}

/** Scroll positions of documents whose editor was unmounted (tab switches). */
const scrollPositions = new Map<number, { top: number; left: number }>()

interface EditorViewProps {
  documentId: number
  title: string
  transport?: EditorTransport
}

/**
 * Renders one document. Only lines near the visible area exist in the DOM;
 * the backend owns the text, so every edit and cursor move is an operation
 * sent through an `EditorSession`.
 */
export function EditorView({ documentId, title, transport = ipcTransport }: EditorViewProps) {
  const frameRef = useRef<HTMLDivElement>(null)
  const scrollRef = useRef<HTMLDivElement>(null)
  const gutterRef = useRef<HTMLDivElement>(null)
  const linesRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLTextAreaElement>(null)
  const sessionRef = useRef<EditorSession | null>(null)
  const viewRef = useRef<View | null>(null)
  const findRef = useRef<FindState | null>(null)
  const composingRef = useRef(false)
  const revealRef = useRef(false)
  const restoreScrollRef = useRef(scrollPositions.get(documentId) ?? null)

  const { lineHeight } = useStore(appearance)
  const lineHeightRef = useRef(lineHeight)
  lineHeightRef.current = lineHeight
  const [view, setView] = useState<View | null>(null)
  const [overlay, setOverlay] = useState<Overlay>(EMPTY_OVERLAY)
  const [composing, setComposing] = useState(false)
  const [find, setFind] = useState<FindState | null>(null)
  const [hover, setHover] = useState<HoverState | null>(null)
  const [completion, setCompletion] = useState<CompletionState | null>(null)
  /** Re-renders popups anchored to the text when it scrolls. */
  const [, setScrollTick] = useState(0)
  viewRef.current = view
  findRef.current = find
  const mirror = useMemo(() => (view ? inputMirror(view) : null) ?? EMPTY_MIRROR, [view])
  const mirrorRef = useRef(mirror)
  mirrorRef.current = mirror
  useLayoutEffect(() => {
    if (inputRef.current && !composing) {
      showMirror(inputRef.current, mirror)
    }
  }, [mirror, composing])
  const hoverRef = useRef(hover)
  hoverRef.current = hover
  const hoverTimerRef = useRef<number | undefined>(undefined)
  /** Bumped to discard hover answers that arrive too late. */
  const hoverSeqRef = useRef(0)
  const completionRef = useRef(completion)
  completionRef.current = completion
  const completionSeqRef = useRef(0)
  /** Completion to request once the view reflects the latest typing. */
  const completionPendingRef = useRef<{ trigger: string | null } | null>(null)
  const [link, setLink] = useState<LinkState | null>(null)
  /** The last pointer position over the text; `null` once it leaves. */
  const pointerRef = useRef<{ x: number; y: number } | null>(null)
  /** The word whose definition was last asked for, to ask once per word. */
  const linkKeyRef = useRef<string | null>(null)
  const linkSeqRef = useRef(0)
  /** When the view being rendered arrived, and when the oldest unpainted input happened. */
  const receivedAtRef = useRef(0)
  const inputStartRef = useRef<number | null>(null)

  // Focusing must not scroll: the input spans the cursor's whole line.
  const focusInput = () => inputRef.current?.focus({ preventScroll: true })

  const dispatch = (op: EditorOp) => {
    revealRef.current = true
    inputStartRef.current ??= performance.now()
    sessionRef.current?.dispatch(op)
  }

  useEffect(() => {
    // A selection requested when the document was opened (e.g. from a search
    // result). It is cleared only once a view reflecting it has arrived, so
    // a mount that is torn down early (React StrictMode) does not lose it.
    const pending = pendingSelection(documentId)
    const session = new EditorSession(
      documentId,
      viewportFor(scrollRef.current, lineHeightRef.current),
      next => {
        receivedAtRef.current = performance.now()
        setView(next)
        if (pending) {
          clearPendingSelection(documentId, pending)
        }
      },
      error => reportError('Editing', error),
      transport,
    )
    sessionRef.current = session
    const unregister = registerBeforeSave(documentId, () => session.idle())
    if (pending) {
      restoreScrollRef.current = null
      revealRef.current = true
      session.dispatch({ type: 'setCursor', position: pending.start, extend: false, add: false })
      session.dispatch({ type: 'setCursor', position: pending.end, extend: true, add: false })
    }
    focusInput()
    return () => {
      const scroller = scrollRef.current
      if (scroller) {
        scrollPositions.set(documentId, { top: scroller.scrollTop, left: scroller.scrollLeft })
      }
      unregister()
      session.dispose()
      sessionRef.current = null
      editorStatus.set(null)
    }
  }, [documentId, transport])

  // Commands reach this editor through the active-editor handle.
  useEffect(() => {
    const controller: EditorController = {
      documentId,
      dispatch,
      openFind: replace => openFind(replace),
      findNext: backward => findNext(backward),
      refresh: () => sessionRef.current?.refresh(),
      focus: () => focusInput(),
      lineCount: () => viewRef.current?.lineCount ?? 0,
      scrollToLine: line => {
        const scroller = scrollRef.current
        if (scroller) {
          scroller.scrollTop = line * lineHeightRef.current
        }
      },
      goToDefinition: () => void goToDefinition(),
      triggerSuggest: () => requestCompletion(null, false),
      showHover: () => void showHoverAtCursor(),
    }
    activeEditor.set(controller)
    return () => {
      if (activeEditor.get() === controller) {
        activeEditor.set(null)
      }
    }
    // The controller reads current state through refs, so it only needs to
    // change with the document.
  }, [documentId])

  // A different font size changes which lines are visible.
  useEffect(() => {
    sessionRef.current?.setViewport(viewportFor(scrollRef.current, lineHeight))
  }, [lineHeight])

  const findQuery = find?.query ?? null
  useEffect(() => {
    sessionRef.current?.setSearch(findQuery)
  }, [findQuery])

  useEffect(() => {
    if (!view) {
      return
    }
    setDirty(documentId, view.dirty)
    editorStatus.set({
      line: view.primary.position.line + 1,
      column: view.primary.visualColumn + 1,
      selectionCount: view.selectionCount,
      language: view.language,
    })
    const pending = completionPendingRef.current
    if (pending) {
      completionPendingRef.current = null
      void fetchCompletion(view.primary.position, pending.trigger)
    }
  }, [view, documentId])

  useEffect(() => () => window.clearTimeout(hoverTimerRef.current), [])

  useLayoutEffect(() => {
    const lines = linesRef.current
    const scroller = scrollRef.current
    if (!view || !lines || !scroller) {
      return
    }
    const measured = measureOverlay(
      lines,
      view.startLine,
      view.selections,
      view.search?.matches ?? [],
      lineHeight,
      view.diagnostics,
      view.decorations,
    )
    setOverlay(measured)
    recordTiming('editor.render', performance.now() - receivedAtRef.current)
    const painted = {
      documentId,
      version: view.version,
      startLine: view.startLine,
      endLine: view.endLine,
    }
    const frame = requestAnimationFrame(() => {
      if (inputStartRef.current !== null) {
        recordTiming('editor.inputToPaint', performance.now() - inputStartRef.current)
        inputStartRef.current = null
      }
      notifyPainted(painted)
    })
    const saved = restoreScrollRef.current
    if (saved) {
      restoreScrollRef.current = null
      scroller.scrollTop = saved.top
      scroller.scrollLeft = saved.left
    } else if (revealRef.current) {
      revealRef.current = false
      const caret = measured.carets.find(c => c.primary)
      reveal(
        scroller,
        view.primary.position.line * lineHeight,
        lineHeight,
        caret?.left,
        gutterRef.current?.offsetWidth ?? 0,
      )
    }
    return () => cancelAnimationFrame(frame)
  }, [view, lineHeight, documentId])

  const primarySelection = () => viewRef.current?.selections.find(s => s.primary)

  const openFind = (replace: boolean) => {
    const current = viewRef.current
    const primary = primarySelection()
    let prefill: string | null = null
    if (current && primary && primary.anchor.line === primary.head.line) {
      const text = current.lines[primary.head.line - current.startLine]?.text
      const from = Math.min(primary.anchor.column, primary.head.column)
      const to = Math.max(primary.anchor.column, primary.head.column)
      if (text !== undefined && to > from) {
        prefill = text.slice(from, to)
      }
    }
    setFind(state => ({
      query: {
        ...(state?.query ?? EMPTY_QUERY),
        ...(prefill !== null ? { pattern: prefill } : {}),
      },
      replacement: state?.replacement ?? '',
      showReplace: replace || (state?.showReplace ?? false),
      focusRequest: (state?.focusRequest ?? 0) + 1,
    }))
  }

  const findNext = (backward: boolean) => {
    const state = findRef.current
    if (!state?.query.pattern) {
      openFind(false)
      return
    }
    dispatch({ type: 'findNext', query: state.query, backward })
  }

  const onFindQueryChange = (change: Partial<SearchQuery>) => {
    const state = findRef.current
    if (!state) {
      return
    }
    const query = { ...state.query, ...change }
    setFind({ ...state, query })
    // Find as you type: jump to the first match at or after the selection start.
    const primary = primarySelection()
    if (query.pattern && primary) {
      dispatch({
        type: 'setCursor',
        position: earlier(primary.anchor, primary.head),
        extend: false,
        add: false,
      })
      dispatch({ type: 'findNext', query, backward: false })
    }
  }

  const closeFind = () => {
    setFind(null)
    focusInput()
  }

  // Scroll events already arrive at most once per frame; requesting lines
  // right away (instead of on the next animation frame) saves a frame per
  // page of scrolling. The session ignores repeated identical viewports.
  const onScroll = () => {
    if (hoverRef.current) {
      setHover(null)
    }
    // The text moved under the pointer; the next pointer move looks again.
    clearLink()
    if (completionRef.current) {
      setScrollTick(tick => tick + 1)
    }
    const scroller = scrollRef.current
    if (scroller && !covers(viewRef.current, scroller, lineHeight)) {
      sessionRef.current?.setViewport(viewportFor(scroller, lineHeight))
    }
  }

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.nativeEvent.isComposing || composingRef.current) {
      return
    }
    hideHover()
    if (shownCompletions && shownCompletions.length > 0 && completion) {
      const plain = !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey
      const count = shownCompletions.length
      const selected = Math.min(completion.selected, count - 1)
      const select = (index: number) => {
        event.preventDefault()
        setCompletion({ ...completion, selected: Math.max(0, Math.min(count - 1, index)) })
      }
      switch (plain ? event.key : '') {
        case 'ArrowDown':
          select((selected + 1) % count)
          return
        case 'ArrowUp':
          select((selected - 1 + count) % count)
          return
        case 'PageDown':
          select(selected + COMPLETION_ROWS)
          return
        case 'PageUp':
          select(selected - COMPLETION_ROWS)
          return
        case 'Enter':
        case 'Tab':
          event.preventDefault()
          acceptCompletion(shownCompletions[selected]!)
          return
        case 'Escape':
          event.preventDefault()
          closeCompletion()
          return
      }
    }
    if (event.key === 'Escape' && findRef.current) {
      event.preventDefault()
      closeFind()
      return
    }
    const pageLines = Math.max(
      1,
      Math.floor((scrollRef.current?.clientHeight ?? 0) / lineHeight) - 1,
    )
    const command = keyToCommand(event, isMac(), pageLines)
    if (!command) {
      return
    }
    event.preventDefault()
    if (command.kind === 'op') {
      dispatch(command.op)
    } else if (command.kind === 'clipboard') {
      revealRef.current = true
      sessionRef.current?.clipboard(command.action)
    }
  }

  const insertTyped = (text: string) => {
    dispatch({ type: 'insert', text })
    afterTyping(text)
  }

  // Typing is applied to the document, not the input: the input mirrors the
  // view (see inputMirror.ts). Keys and clipboard events are handled
  // before they reach `beforeinput`; composition text can't be cancelled.
  useEffect(() => {
    const input = inputRef.current
    if (!input) {
      return
    }
    const onBeforeInput = (event: InputEvent) => {
      if (event.isComposing || composingRef.current) {
        return
      }
      if (event.inputType === 'insertCompositionText') {
        return
      }
      event.preventDefault()
      if (
        (event.inputType === 'insertText' || event.inputType === 'insertReplacementText') &&
        event.data
      ) {
        insertTyped(event.data)
      }
    }
    input.addEventListener('beforeinput', onBeforeInput)
    return () => input.removeEventListener('beforeinput', onBeforeInput)
  })

  // Input the browser applied anyway: apply the difference, then restore.
  const onInput = (event: FormEvent<HTMLTextAreaElement>) => {
    if ((event.nativeEvent as InputEvent).isComposing || composingRef.current) {
      return
    }
    const input = event.currentTarget
    const current = mirrorRef.current
    const typed = typedText(current, input.value)
    showMirror(input, current)
    if (typed) {
      insertTyped(typed)
    }
  }

  const onCompositionStart = () => {
    composingRef.current = true
    setComposing(true)
  }

  const onCompositionEnd = (event: CompositionEvent<HTMLTextAreaElement>) => {
    composingRef.current = false
    setComposing(false)
    showMirror(event.currentTarget, mirrorRef.current)
    if (event.data) {
      insertTyped(event.data)
    }
  }

  const onPaste = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    event.preventDefault()
    const text = event.clipboardData.getData('text/plain')
    if (text) {
      dispatch({ type: 'insert', text })
    }
  }

  const onClipboardEvent = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    event.preventDefault()
    sessionRef.current?.clipboard(event.type === 'cut' ? 'cut' : 'copy')
  }

  const positionAt = (clientX: number, clientY: number): Position | null => {
    const lines = linesRef.current
    const current = viewRef.current
    if (!lines || !current) {
      return null
    }
    const top = lines.getBoundingClientRect().top
    const row = Math.floor((clientY - top) / lineHeight)
    const line = current.startLine + row
    if (line < 0) {
      return { line: 0, column: 0 }
    }
    if (line >= current.lineCount) {
      return { line: current.lineCount - 1, column: END_OF_LINE }
    }
    const element = lines.children[line - current.startLine]
    const column = element ? columnAt(element, clientX, top + (row + 0.5) * lineHeight) : 0
    return { line, column }
  }

  /**
   * Whether a point is below the last line, in the space that fills the
   * view when the document is short. Clicks there go to the end of the last
   * line, but nothing is hovered or linked.
   */
  const belowText = (clientY: number): boolean => {
    const lines = linesRef.current
    const current = viewRef.current
    if (!lines || !current) {
      return false
    }
    const textBottom =
      lines.getBoundingClientRect().top + (current.lineCount - current.startLine) * lineHeight
    return clientY >= textBottom
  }

  const onMouseDown = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return
    }
    event.preventDefault()
    focusInput()
    hideHover()
    closeCompletion()
    const position = positionAt(event.clientX, event.clientY)
    if (!position) {
      return
    }
    if (definitionModifier(event)) {
      dispatch({ type: 'setCursor', position, extend: false, add: false })
      if (!belowText(event.clientY)) {
        void goToDefinition(position)
      }
      return
    }
    if (event.detail === 2) {
      dispatch({ type: 'selectWord', position })
      return
    }
    dispatch({ type: 'setCursor', position, extend: event.shiftKey, add: event.altKey })

    const onMove = (move: MouseEvent) => {
      const to = positionAt(move.clientX, move.clientY)
      if (to) {
        dispatch({ type: 'setCursor', position: to, extend: true, add: false })
      }
    }
    const onUp = () => {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
    }
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }

  const hideHover = () => {
    window.clearTimeout(hoverTimerRef.current)
    hoverSeqRef.current++
    if (hoverRef.current) {
      setHover(null)
    }
  }

  /** Where a popup of `height` goes: below `line`, or above it if there is no room. */
  const placeAt = (line: number, left: number, height: number): Placement | null => {
    const frame = frameRef.current
    const lines = linesRef.current
    const current = viewRef.current
    if (!frame || !lines || !current) {
      return null
    }
    const frameBox = frame.getBoundingClientRect()
    const lineTop =
      lines.getBoundingClientRect().top - frameBox.top + (line - current.startLine) * lineHeight
    const x = Math.max(0, Math.min(left, frameBox.width - COMPLETION_WIDTH))
    const fitsBelow = lineTop + lineHeight + height <= frameBox.height
    return fitsBelow || lineTop < height
      ? { top: lineTop + lineHeight, left: x }
      : { bottom: frameBox.height - lineTop, left: x }
  }

  const showHoverAt = async (position: Position, x: number) => {
    const seq = ++hoverSeqRef.current
    let result: lsp.Hover | null = null
    try {
      result = await lsp.hover(documentId, position)
    } catch (error) {
      console.debug('hover failed', error)
    }
    const frame = frameRef.current
    if (seq !== hoverSeqRef.current || !result || !frame) {
      return
    }
    const left = x - frame.getBoundingClientRect().left
    // Hover cards vary in height: show below the line in the upper half.
    const placement = placeAt(position.line, left, frame.clientHeight / 2)
    if (placement) {
      setHover({ hover: result, line: position.line, x, placement })
    }
  }

  const showHoverAtCursor = async () => {
    const current = viewRef.current
    const caret = overlay.carets.find(c => c.primary)
    const lines = linesRef.current
    if (current && caret && lines) {
      await showHoverAt(current.primary.position, lines.getBoundingClientRect().left + caret.left)
    }
  }

  const onContentMouseMove = (event: ReactMouseEvent<HTMLDivElement>) => {
    const { clientX, clientY } = event
    pointerRef.current = { x: clientX, y: clientY }
    updateLink(event.buttons === 0 && definitionModifier(event))
    if (event.buttons !== 0) {
      return
    }
    const position = positionAt(clientX, clientY)
    const shown = hoverRef.current
    if (shown && position?.line === shown.line && Math.abs(clientX - shown.x) < HOVER_SLACK) {
      return
    }
    hideHover()
    // Hover shows language information and diagnostics (which extensions
    // may publish for any file).
    const current = viewRef.current
    if (!position || !(current?.language || current?.diagnostics.length) || belowText(clientY)) {
      return
    }
    hoverTimerRef.current = window.setTimeout(() => {
      // Only over text, not the empty space after a line.
      const lines = linesRef.current
      const element = lines?.children[position.line - (viewRef.current?.startLine ?? 0)]
      if (!lines || !element) {
        return
      }
      const end = lines.getBoundingClientRect().left + columnX(element, lineLength(element))
      if (clientX <= end) {
        void showHoverAt(position, clientX)
      }
    }, HOVER_DELAY)
  }

  /** The editor's context menu: clipboard, language, extension, and palette entries. */
  const onContextMenu = (event: ReactMouseEvent<HTMLDivElement>) => {
    event.preventDefault()
    hideHover()
    closeCompletion()
    focusInput()
    // Shift+F10 and the menu key target the focused input (the mouse can't
    // reach it): open the menu at the cursor, without moving it.
    const fromKeyboard = event.target === inputRef.current
    const position = fromKeyboard ? null : positionAt(event.clientX, event.clientY)
    const current = viewRef.current
    let [x, y] = [event.clientX, event.clientY]
    const caret = overlay.carets.find(candidate => candidate.primary)
    const lines = linesRef.current?.getBoundingClientRect()
    if (fromKeyboard && caret && lines) {
      ;[x, y] = [lines.left + caret.left, lines.top + caret.top + lineHeight]
    }
    // Right-clicking outside the selection moves the cursor there first.
    if (position && current) {
      const inside = current.selections.some(selection => {
        const [start, end] = [
          earlier(selection.anchor, selection.head),
          later(selection.anchor, selection.head),
        ]
        return (
          !samePosition(start, end) && !earlierThan(position, start) && !earlierThan(end, position)
        )
      })
      if (!inside) {
        dispatch({ type: 'setCursor', position, extend: false, add: false })
      }
    }
    const hint = (id: string) => primaryKeybinding(getCommand(id))
    const clipboard = (action: 'cut' | 'copy' | 'paste') => () => {
      revealRef.current = true
      sessionRef.current?.clipboard(action)
    }
    const entries: MenuEntry[] = []
    if (current?.language) {
      entries.push(
        {
          label: 'Go to Definition',
          hint: hint('editor.goToDefinition'),
          run: () => goToDefinition(position ?? undefined),
        },
        null,
      )
    }
    entries.push(
      { label: 'Cut', hint: isMac() ? '⌘X' : 'Ctrl+X', run: clipboard('cut') },
      { label: 'Copy', hint: isMac() ? '⌘C' : 'Ctrl+C', run: clipboard('copy') },
      { label: 'Paste', hint: isMac() ? '⌘V' : 'Ctrl+V', run: clipboard('paste') },
      null,
      ...contributedMenuItems('editor/context').map(item => ({
        label: item.title,
        hint: hint(item.command),
        run: () => runContributed(item.command),
      })),
      null,
      {
        label: 'Command Palette…',
        hint: hint('workbench.commandPalette'),
        run: () => openPalette('>'),
      },
    )
    openContextMenu(x, y, entries)
  }

  const onContentMouseLeave = (event: ReactMouseEvent<HTMLDivElement>) => {
    pointerRef.current = null
    clearLink()
    const to = event.relatedTarget
    const card = frameRef.current?.querySelector('.hover-card')
    if (!(to instanceof Node && card?.contains(to))) {
      hideHover()
    }
  }

  const goToDefinition = async (position?: Position) => {
    const at = position ?? viewRef.current?.primary.position
    if (!at) {
      return
    }
    try {
      const location = await lsp.definition(documentId, at)
      if (location) {
        await openPath(location.path, { start: location.start })
      } else {
        notify('info', 'No definition found.')
      }
    } catch (error) {
      reportError('Going to the definition', error)
    }
  }

  /** The word under a point of the text, if any. */
  const wordAt = (clientX: number, clientY: number): Word | null => {
    const position = positionAt(clientX, clientY)
    const lines = linesRef.current
    const current = viewRef.current
    if (!position || !lines || !current || belowText(clientY)) {
      return null
    }
    const element = lines.children[position.line - current.startLine]
    const text = current.lines[position.line - current.startLine]?.text
    if (!element || text === undefined) {
      return null
    }
    // `positionAt` finds the nearest caret boundary; left of it, the
    // pointer is over the character before it.
    const boundary = lines.getBoundingClientRect().left + columnX(element, position.column)
    const index = clientX < boundary ? position.column - 1 : position.column
    const word = wordAround(text, index)
    return word && { line: position.line, from: word[0], to: word[1] }
  }

  const clearLink = () => {
    linkSeqRef.current++
    linkKeyRef.current = null
    setLink(null)
  }

  /**
   * Draws the word under the pointer as a link while `held` (the
   * go-to-definition modifier is down), once the server confirms it has a
   * definition, as VS Code does.
   */
  const updateLink = (held: boolean) => {
    const pointer = pointerRef.current
    const current = viewRef.current
    const word = held && pointer && current?.language ? wordAt(pointer.x, pointer.y) : null
    const key = word && current ? `${current.version}:${word.line}:${word.from}` : null
    if (key === linkKeyRef.current) {
      return
    }
    clearLink()
    if (!word || !current) {
      return
    }
    linkKeyRef.current = key
    const seq = linkSeqRef.current
    const version = current.version
    lsp.definition(documentId, { line: word.line, column: word.from }).then(
      location => {
        if (location && seq === linkSeqRef.current) {
          setLink({ ...word, version })
        }
      },
      error => console.debug('definition lookup failed', error),
    )
  }
  const updateLinkRef = useRef(updateLink)
  updateLinkRef.current = updateLink

  // Pressing or releasing the modifier with the pointer over a symbol shows
  // or hides its link, wherever focus is.
  useEffect(() => {
    const onKey = (event: globalThis.KeyboardEvent) =>
      updateLinkRef.current(definitionModifier(event))
    const onBlur = () => updateLinkRef.current(false)
    window.addEventListener('keydown', onKey)
    window.addEventListener('keyup', onKey)
    window.addEventListener('blur', onBlur)
    return () => {
      window.removeEventListener('keydown', onKey)
      window.removeEventListener('keyup', onKey)
      window.removeEventListener('blur', onBlur)
    }
  }, [])

  const closeCompletion = () => {
    completionSeqRef.current++
    completionPendingRef.current = null
    if (completionRef.current) {
      setCompletion(null)
    }
  }

  /**
   * Asks for completions at the cursor: right away, or (`afterEdit`) once
   * the view reflects the edit just dispatched.
   */
  const requestCompletion = (trigger: string | null, afterEdit: boolean) => {
    const current = viewRef.current
    if (!current?.language?.completionTriggers) {
      return
    }
    if (afterEdit) {
      completionPendingRef.current = { trigger }
    } else {
      void fetchCompletion(current.primary.position, trigger)
    }
  }

  const fetchCompletion = async (position: Position, trigger: string | null) => {
    const seq = ++completionSeqRef.current
    let result: lsp.Completions | null = null
    try {
      result = await lsp.completion(documentId, position, trigger)
    } catch (error) {
      console.debug('completion failed', error)
    }
    if (seq !== completionSeqRef.current) {
      return
    }
    setCompletion(
      result && result.items.length > 0
        ? { items: result.items, incomplete: result.incomplete, selected: 0 }
        : null,
    )
  }

  /** Opens, refines, or closes completion after `text` was typed. */
  const afterTyping = (text: string) => {
    const triggers = viewRef.current?.language?.completionTriggers
    const last = text.slice(-1)
    if (!triggers || !last) {
      return
    }
    const open = completionRef.current
    if (triggers.includes(last)) {
      requestCompletion(last, true)
    } else if (isWordChar(last)) {
      if (!open || open.incomplete) {
        requestCompletion(null, true)
      }
    } else {
      closeCompletion()
    }
  }

  const acceptCompletion = (item: lsp.CompletionItem) => {
    // Select from the item's start to the cursor, then replace it.
    dispatch({ type: 'setCursor', position: item.from, extend: true, add: false })
    dispatch({ type: 'insert', text: item.insert })
    closeCompletion()
    focusInput()
  }

  const primaryCaret: Rect | undefined = overlay.carets.find(c => c.primary)
  const cursorLine = view?.primary.position.line ?? 0
  const shownCompletions =
    completion && view
      ? visibleCompletions(
          completion.items,
          view.primary.position,
          view.lines[cursorLine - view.startLine]?.text ?? '',
        )
      : null
  const completionLeft =
    (linesRef.current?.getBoundingClientRect().left ?? 0) -
    (frameRef.current?.getBoundingClientRect().left ?? 0) +
    (primaryCaret?.left ?? 0)
  const completionPlacement =
    shownCompletions && shownCompletions.length > 0 && primaryCaret
      ? placeAt(
          cursorLine,
          completionLeft,
          Math.min(shownCompletions.length, COMPLETION_ROWS) * COMPLETION_ROW_HEIGHT + 2,
        )
      : null

  const startLine = view?.startLine ?? 0
  const lineCount = view?.lineCount ?? 1
  const extensions = useStore(extensionsStore)
  const kind = useStore(themeKind)
  const types = useMemo(() => decorationTypes(extensions), [extensions])
  const lineOverlays = useMemo(
    () =>
      view && view.decorations.length > 0
        ? textOverlays(
            view.decorations,
            types,
            kind,
            view.startLine,
            view.lines.map(line => line.text.length),
          )
        : new Map<number, TextOverlay[]>(),
    [view, types, kind],
  )
  const primary = primaryCaret
  const activeLine = view?.primary.position.line
  const layerStyle = { transform: `translateY(${startLine * lineHeight}px)` }

  return (
    <div className="editor-view-frame" ref={frameRef}>
      {find && (
        <FindWidget
          state={find}
          result={view?.search ?? null}
          onQueryChange={onFindQueryChange}
          onReplacementChange={replacement => setFind({ ...find, replacement })}
          onToggleReplace={() => setFind({ ...find, showReplace: !find.showReplace })}
          onNext={findNext}
          onReplace={() =>
            dispatch({ type: 'replace', query: find.query, replacement: find.replacement })
          }
          onReplaceAll={() =>
            dispatch({ type: 'replaceAll', query: find.query, replacement: find.replacement })
          }
          onClose={closeFind}
        />
      )}
      <div className="editor-view" ref={scrollRef} onScroll={onScroll}>
        {/* The lines plus a view's height less one line, so the last line can
            scroll to the top (as in VS Code). Clicks in that space reach the
            content and go to the end of the last line. */}
        <div
          className="editor-view__sizer"
          style={{
            minHeight: `max(${lineCount * lineHeight}px, 100% + ${(lineCount - 1) * lineHeight}px)`,
          }}
        >
          <div
            className="editor-view__gutter"
            ref={gutterRef}
            style={{ width: `calc(${String(lineCount).length}ch + 32px)` }}
            aria-hidden="true"
          >
            <div style={layerStyle}>
              {view?.lines.map((_, index) => (
                <div
                  key={startLine + index}
                  className="editor-view__line-number"
                  data-active={startLine + index === activeLine || undefined}
                >
                  {startLine + index + 1}
                </div>
              ))}
            </div>
          </div>
          <div
            className="editor-view__content"
            onMouseDown={onMouseDown}
            onMouseMove={onContentMouseMove}
            onMouseLeave={onContentMouseLeave}
            onContextMenu={onContextMenu}
          >
            <div className="editor-view__layer" style={layerStyle}>
              <div className="editor-view__selections" aria-hidden="true">
                {overlay.matches.map((rect, index) => (
                  <div
                    key={`m${index}`}
                    className="editor-view__match"
                    data-current={rect.current || undefined}
                    style={{ top: rect.top, left: rect.left, width: rect.width }}
                  />
                ))}
                {overlay.decorations.map((rect, index) => {
                  const type = types.get(rect.key)
                  return (
                    type && (
                      <div
                        key={`x${index}`}
                        className="editor-view__decoration"
                        data-underline={type.underline ?? undefined}
                        style={{
                          ...decorationRectStyle(type, kind),
                          top: rect.top,
                          left: rect.left,
                          width: rect.width,
                        }}
                      />
                    )
                  )
                })}
                {overlay.selections.map((rect, index) => (
                  <div
                    key={index}
                    className="editor-view__selection"
                    style={{ top: rect.top, left: rect.left, width: rect.width }}
                  />
                ))}
                {overlay.diagnostics.map((rect, index) => (
                  <div
                    key={`d${index}`}
                    className="editor-view__diagnostic"
                    data-severity={severityName(rect.severity)}
                    style={{ top: rect.top, left: rect.left, width: rect.width }}
                  />
                ))}
              </div>
              <div className="editor-view__lines" ref={linesRef}>
                {view?.lines.map((line, index) => (
                  <Line
                    key={startLine + index}
                    line={line}
                    link={
                      link?.line === startLine + index && link.version === view.version
                        ? link
                        : null
                    }
                    overlays={lineOverlays.get(startLine + index) ?? NO_OVERLAYS}
                  />
                ))}
              </div>
              <div className="editor-view__carets" aria-hidden="true">
                {overlay.carets.map((caret, index) => (
                  <div
                    key={index}
                    className="editor-view__caret"
                    style={{ top: caret.top, left: caret.left }}
                  />
                ))}
              </div>
              <textarea
                ref={inputRef}
                className="editor-view__input"
                data-editor-input=""
                data-composing={composing || undefined}
                data-mirror={mirror.line >= 0 || undefined}
                style={
                  mirror.line >= 0
                    ? {
                        top: (mirror.line - startLine) * lineHeight,
                        left: 0,
                        width: '100%',
                        height: mirror.value.split('\n').length * lineHeight,
                      }
                    : { top: primary?.top ?? 0, left: primary?.left ?? 0 }
                }
                aria-label={`Editor for ${title}`}
                aria-multiline="true"
                autoComplete="off"
                autoCorrect="off"
                autoCapitalize="off"
                spellCheck={false}
                wrap="off"
                onKeyDown={onKeyDown}
                onInput={onInput}
                onCompositionStart={onCompositionStart}
                onCompositionEnd={onCompositionEnd}
                onPaste={onPaste}
                onCopy={onClipboardEvent}
                onCut={onClipboardEvent}
              />
            </div>
          </div>
        </div>
      </div>
      {hover && (
        <HoverCard hover={hover.hover} placement={hover.placement} onMouseLeave={hideHover} />
      )}
      {shownCompletions && completion && completionPlacement && (
        <CompletionList
          items={shownCompletions}
          selected={Math.min(completion.selected, shownCompletions.length - 1)}
          placement={completionPlacement}
          onPick={acceptCompletion}
        />
      )}
    </div>
  )
}

function earlier(a: Position, b: Position): Position {
  return a.line < b.line || (a.line === b.line && a.column <= b.column) ? a : b
}

const NO_OVERLAYS: readonly TextOverlay[] = []

/** The input's content when nothing is mirrored: empty, at the caret. */
const EMPTY_MIRROR: InputMirror = {
  line: -1,
  value: '',
  selectionStart: 0,
  selectionEnd: 0,
  direction: 'forward',
}

function showMirror(input: HTMLTextAreaElement, mirror: InputMirror): void {
  if (input.value !== mirror.value) {
    input.value = mirror.value
  }
  if (
    input.selectionStart !== mirror.selectionStart ||
    input.selectionEnd !== mirror.selectionEnd ||
    input.selectionDirection !== mirror.direction
  ) {
    input.setSelectionRange(mirror.selectionStart, mirror.selectionEnd, mirror.direction)
  }
}

interface LineProps {
  line: DisplayLine
  link: Word | null
  /** Decoration styles on this line. */
  overlays: readonly TextOverlay[]
}

const Line = memo(function Line({ line, link, overlays }: LineProps) {
  const all = link
    ? [...overlays, { from: link.from, to: link.to, className: LINK_CLASS }]
    : overlays
  const text =
    line.spans.length === 0 && all.length === 0
      ? line.text
      : highlighted(line.text, line.spans, all)
  if (line.fullLength === null) {
    return <div className="editor-view__line">{text}</div>
  }
  // A very long line shortened for display; the document keeps all of it.
  return (
    <div className="editor-view__line">
      {text}
      <span
        className="editor-view__cut"
        title={`Line shortened for display (${line.fullLength.toLocaleString()} characters)`}
      >
        …
      </span>
    </div>
  )
})

/** `text` split into plain, highlighted, decorated, and linked runs (see `measure.ts`). */
function highlighted(
  text: string,
  spans: readonly number[],
  overlays: readonly TextOverlay[],
): ReactNode[] {
  return segments(text, spans, overlays).map(({ start, end, className, style }) =>
    className || style ? (
      <span key={start} className={className || undefined} style={style}>
        {text.slice(start, end)}
      </span>
    ) : (
      text.slice(start, end)
    ),
  )
}

function later(a: Position, b: Position): Position {
  return earlier(a, b) === a ? b : a
}

function earlierThan(a: Position, b: Position): boolean {
  return a.line < b.line || (a.line === b.line && a.column < b.column)
}

function samePosition(a: Position, b: Position): boolean {
  return a.line === b.line && a.column === b.column
}

function visibleRange(scroller: HTMLElement, lineHeight: number): [number, number] {
  const first = Math.floor(scroller.scrollTop / lineHeight)
  const count = Math.ceil(scroller.clientHeight / lineHeight) + 1
  return [first, first + count]
}

function viewportFor(scroller: HTMLElement | null, lineHeight: number): Viewport {
  if (!scroller) {
    return { startLine: 0, endLine: OVERSCAN * 2 }
  }
  const [first, last] = visibleRange(scroller, lineHeight)
  return { startLine: Math.max(0, first - OVERSCAN), endLine: last + OVERSCAN }
}

/** Whether `view` already contains every visible line. */
function covers(view: View | null, scroller: HTMLElement, lineHeight: number): boolean {
  if (!view) {
    return false
  }
  const [first, last] = visibleRange(scroller, lineHeight)
  return (
    view.startLine <= first && view.startLine + view.lines.length >= Math.min(last, view.lineCount)
  )
}

/** Scrolls just enough to show the primary caret. */
function reveal(
  scroller: HTMLElement,
  top: number,
  lineHeight: number,
  left: number | undefined,
  gutterWidth: number,
): void {
  if (top < scroller.scrollTop) {
    scroller.scrollTop = top
  } else if (top + lineHeight > scroller.scrollTop + scroller.clientHeight) {
    scroller.scrollTop = top + lineHeight - scroller.clientHeight
  }
  if (left === undefined) {
    return
  }
  const margin = 2 * lineHeight
  const textWidth = scroller.clientWidth - gutterWidth
  if (left < scroller.scrollLeft) {
    scroller.scrollLeft = Math.max(0, left - margin)
  } else if (left > scroller.scrollLeft + textWidth - margin) {
    scroller.scrollLeft = left - textWidth + margin
  }
}
