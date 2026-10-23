import {
  memo,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ClipboardEvent,
  type CompositionEvent,
  type FormEvent,
  type KeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";
import {
  TOKEN_KINDS,
  type DisplayLine,
  type EditorOp,
  type EditorView as View,
  type Position,
  type SearchQuery,
  type Viewport,
} from "../ipc/editor";
import * as lsp from "../ipc/lsp";
import { appearance } from "../state/appearance";
import {
  clearPendingSelection,
  openPath,
  pendingSelection,
  registerBeforeSave,
  setDirty,
} from "../state/documents";
import { editorStatus } from "../state/editorStatus";
import { notify, reportError } from "../state/notifications";
import { notifyPainted, recordTiming } from "../state/perf";
import { useStore } from "../state/store";
import { activeEditor, type EditorController } from "./activeEditor";
import { isWordChar, visibleCompletions } from "./completion";
import { FindWidget, type FindState } from "./FindWidget";
import { isMac, keyToCommand } from "./keymap";
import {
  COMPLETION_ROW_HEIGHT,
  COMPLETION_ROWS,
  COMPLETION_WIDTH,
  CompletionList,
  HoverCard,
  severityName,
  type Placement,
} from "./LanguagePopups";
import {
  columnAt,
  columnX,
  EMPTY_OVERLAY,
  lineLength,
  measureOverlay,
  type Overlay,
  type Rect,
} from "./measure";
import { EditorSession, ipcTransport, type EditorTransport } from "./session";
import "./EditorView.css";

/** Extra lines fetched above and below the visible area. */
const OVERSCAN = 20;
/** A column beyond any line end; the backend clamps it to the line end. */
const END_OF_LINE = Number.MAX_SAFE_INTEGER;
const EMPTY_QUERY: SearchQuery = {
  pattern: "",
  regex: false,
  caseSensitive: false,
  wholeWord: false,
};

/** How long the pointer rests on a symbol before its hover card shows. */
const HOVER_DELAY = 500;
/** How far the pointer may drift along a line while a hover card stays. */
const HOVER_SLACK = 40;

interface HoverState {
  hover: lsp.Hover;
  line: number;
  x: number;
  placement: Placement;
}

interface CompletionState {
  items: lsp.CompletionItem[];
  incomplete: boolean;
  selected: number;
}

/** Scroll positions of documents whose editor was unmounted (tab switches). */
const scrollPositions = new Map<number, { top: number; left: number }>();

interface EditorViewProps {
  documentId: number;
  title: string;
  transport?: EditorTransport;
}

/**
 * Renders one document. Only lines near the visible area exist in the DOM;
 * the backend owns the text, so every edit and cursor move is an operation
 * sent through an `EditorSession`.
 */
export function EditorView({ documentId, title, transport = ipcTransport }: EditorViewProps) {
  const frameRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const gutterRef = useRef<HTMLDivElement>(null);
  const linesRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const sessionRef = useRef<EditorSession | null>(null);
  const viewRef = useRef<View | null>(null);
  const findRef = useRef<FindState | null>(null);
  const composingRef = useRef(false);
  const revealRef = useRef(false);
  const restoreScrollRef = useRef(scrollPositions.get(documentId) ?? null);

  const { lineHeight } = useStore(appearance);
  const lineHeightRef = useRef(lineHeight);
  lineHeightRef.current = lineHeight;
  const [view, setView] = useState<View | null>(null);
  const [overlay, setOverlay] = useState<Overlay>(EMPTY_OVERLAY);
  const [composing, setComposing] = useState(false);
  const [find, setFind] = useState<FindState | null>(null);
  const [hover, setHover] = useState<HoverState | null>(null);
  const [completion, setCompletion] = useState<CompletionState | null>(null);
  /** Re-renders popups anchored to the text when it scrolls. */
  const [, setScrollTick] = useState(0);
  viewRef.current = view;
  findRef.current = find;
  const hoverRef = useRef(hover);
  hoverRef.current = hover;
  const hoverTimerRef = useRef<number | undefined>(undefined);
  /** Bumped to discard hover answers that arrive too late. */
  const hoverSeqRef = useRef(0);
  const completionRef = useRef(completion);
  completionRef.current = completion;
  const completionSeqRef = useRef(0);
  /** Completion to request once the view reflects the latest typing. */
  const completionPendingRef = useRef<{ trigger: string | null } | null>(null);
  /** When the view being rendered arrived, and when the oldest unpainted input happened. */
  const receivedAtRef = useRef(0);
  const inputStartRef = useRef<number | null>(null);

  const dispatch = (op: EditorOp) => {
    revealRef.current = true;
    inputStartRef.current ??= performance.now();
    sessionRef.current?.dispatch(op);
  };

  useEffect(() => {
    // A selection requested when the document was opened (e.g. from a search
    // result). It is cleared only once a view reflecting it has arrived, so
    // a mount that is torn down early (React StrictMode) does not lose it.
    const pending = pendingSelection(documentId);
    const session = new EditorSession(
      documentId,
      viewportFor(scrollRef.current, lineHeightRef.current),
      (next) => {
        receivedAtRef.current = performance.now();
        setView(next);
        if (pending) {
          clearPendingSelection(documentId, pending);
        }
      },
      (error) => reportError("Editing", error),
      transport,
    );
    sessionRef.current = session;
    const unregister = registerBeforeSave(documentId, () => session.idle());
    if (pending) {
      restoreScrollRef.current = null;
      revealRef.current = true;
      session.dispatch({ type: "setCursor", position: pending.start, extend: false, add: false });
      session.dispatch({ type: "setCursor", position: pending.end, extend: true, add: false });
    }
    inputRef.current?.focus();
    return () => {
      const scroller = scrollRef.current;
      if (scroller) {
        scrollPositions.set(documentId, { top: scroller.scrollTop, left: scroller.scrollLeft });
      }
      unregister();
      session.dispose();
      sessionRef.current = null;
      editorStatus.set(null);
    };
  }, [documentId, transport]);

  // Commands reach this editor through the active-editor handle.
  useEffect(() => {
    const controller: EditorController = {
      documentId,
      dispatch,
      openFind: (replace) => openFind(replace),
      findNext: (backward) => findNext(backward),
      refresh: () => sessionRef.current?.refresh(),
      focus: () => inputRef.current?.focus(),
      lineCount: () => viewRef.current?.lineCount ?? 0,
      scrollToLine: (line) => {
        const scroller = scrollRef.current;
        if (scroller) {
          scroller.scrollTop = line * lineHeightRef.current;
        }
      },
      goToDefinition: () => void goToDefinition(),
      triggerSuggest: () => requestCompletion(null, false),
      showHover: () => void showHoverAtCursor(),
    };
    activeEditor.set(controller);
    return () => {
      if (activeEditor.get() === controller) {
        activeEditor.set(null);
      }
    };
    // The controller reads current state through refs, so it only needs to
    // change with the document.
  }, [documentId]);

  // A different font size changes which lines are visible.
  useEffect(() => {
    sessionRef.current?.setViewport(viewportFor(scrollRef.current, lineHeight));
  }, [lineHeight]);

  const findQuery = find?.query ?? null;
  useEffect(() => {
    sessionRef.current?.setSearch(findQuery);
  }, [findQuery]);

  useEffect(() => {
    if (!view) {
      return;
    }
    setDirty(documentId, view.dirty);
    editorStatus.set({
      line: view.primary.position.line + 1,
      column: view.primary.visualColumn + 1,
      selectionCount: view.selectionCount,
      language: view.language,
    });
    const pending = completionPendingRef.current;
    if (pending) {
      completionPendingRef.current = null;
      void fetchCompletion(view.primary.position, pending.trigger);
    }
  }, [view, documentId]);

  useEffect(() => () => window.clearTimeout(hoverTimerRef.current), []);

  useLayoutEffect(() => {
    const lines = linesRef.current;
    const scroller = scrollRef.current;
    if (!view || !lines || !scroller) {
      return;
    }
    const measured = measureOverlay(
      lines,
      view.startLine,
      view.selections,
      view.search?.matches ?? [],
      lineHeight,
      view.diagnostics,
    );
    setOverlay(measured);
    recordTiming("editor.render", performance.now() - receivedAtRef.current);
    const painted = {
      documentId,
      version: view.version,
      startLine: view.startLine,
      endLine: view.endLine,
    };
    const frame = requestAnimationFrame(() => {
      if (inputStartRef.current !== null) {
        recordTiming("editor.inputToPaint", performance.now() - inputStartRef.current);
        inputStartRef.current = null;
      }
      notifyPainted(painted);
    });
    const saved = restoreScrollRef.current;
    if (saved) {
      restoreScrollRef.current = null;
      scroller.scrollTop = saved.top;
      scroller.scrollLeft = saved.left;
    } else if (revealRef.current) {
      revealRef.current = false;
      const caret = measured.carets.find((c) => c.primary);
      reveal(
        scroller,
        view.primary.position.line * lineHeight,
        lineHeight,
        caret?.left,
        gutterRef.current?.offsetWidth ?? 0,
      );
    }
    return () => cancelAnimationFrame(frame);
  }, [view, lineHeight, documentId]);

  const primarySelection = () => viewRef.current?.selections.find((s) => s.primary);

  const openFind = (replace: boolean) => {
    const current = viewRef.current;
    const primary = primarySelection();
    let prefill: string | null = null;
    if (current && primary && primary.anchor.line === primary.head.line) {
      const text = current.lines[primary.head.line - current.startLine]?.text;
      const from = Math.min(primary.anchor.column, primary.head.column);
      const to = Math.max(primary.anchor.column, primary.head.column);
      if (text !== undefined && to > from) {
        prefill = text.slice(from, to);
      }
    }
    setFind((state) => ({
      query: {
        ...(state?.query ?? EMPTY_QUERY),
        ...(prefill !== null ? { pattern: prefill } : {}),
      },
      replacement: state?.replacement ?? "",
      showReplace: replace || (state?.showReplace ?? false),
      focusRequest: (state?.focusRequest ?? 0) + 1,
    }));
  };

  const findNext = (backward: boolean) => {
    const state = findRef.current;
    if (!state?.query.pattern) {
      openFind(false);
      return;
    }
    dispatch({ type: "findNext", query: state.query, backward });
  };

  const onFindQueryChange = (change: Partial<SearchQuery>) => {
    const state = findRef.current;
    if (!state) {
      return;
    }
    const query = { ...state.query, ...change };
    setFind({ ...state, query });
    // Find as you type: jump to the first match at or after the selection start.
    const primary = primarySelection();
    if (query.pattern && primary) {
      dispatch({
        type: "setCursor",
        position: earlier(primary.anchor, primary.head),
        extend: false,
        add: false,
      });
      dispatch({ type: "findNext", query, backward: false });
    }
  };

  const closeFind = () => {
    setFind(null);
    inputRef.current?.focus();
  };

  // Scroll events already arrive at most once per frame; requesting lines
  // right away (instead of on the next animation frame) saves a frame per
  // page of scrolling. The session ignores repeated identical viewports.
  const onScroll = () => {
    if (hoverRef.current) {
      setHover(null);
    }
    if (completionRef.current) {
      setScrollTick((tick) => tick + 1);
    }
    const scroller = scrollRef.current;
    if (scroller && !covers(viewRef.current, scroller, lineHeight)) {
      sessionRef.current?.setViewport(viewportFor(scroller, lineHeight));
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.nativeEvent.isComposing || composingRef.current) {
      return;
    }
    hideHover();
    if (shownCompletions && shownCompletions.length > 0 && completion) {
      const plain = !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey;
      const count = shownCompletions.length;
      const selected = Math.min(completion.selected, count - 1);
      const select = (index: number) => {
        event.preventDefault();
        setCompletion({ ...completion, selected: Math.max(0, Math.min(count - 1, index)) });
      };
      switch (plain ? event.key : "") {
        case "ArrowDown":
          select((selected + 1) % count);
          return;
        case "ArrowUp":
          select((selected - 1 + count) % count);
          return;
        case "PageDown":
          select(selected + COMPLETION_ROWS);
          return;
        case "PageUp":
          select(selected - COMPLETION_ROWS);
          return;
        case "Enter":
        case "Tab":
          event.preventDefault();
          acceptCompletion(shownCompletions[selected]!);
          return;
        case "Escape":
          event.preventDefault();
          closeCompletion();
          return;
      }
    }
    if (event.key === "Escape" && findRef.current) {
      event.preventDefault();
      closeFind();
      return;
    }
    const pageLines = Math.max(
      1,
      Math.floor((scrollRef.current?.clientHeight ?? 0) / lineHeight) - 1,
    );
    const command = keyToCommand(event, isMac(), pageLines);
    if (!command) {
      return;
    }
    event.preventDefault();
    if (command.kind === "op") {
      dispatch(command.op);
    } else if (command.kind === "clipboard") {
      revealRef.current = true;
      sessionRef.current?.clipboard(command.action);
    }
  };

  const onInput = (event: FormEvent<HTMLTextAreaElement>) => {
    if ((event.nativeEvent as InputEvent).isComposing || composingRef.current) {
      return;
    }
    const text = event.currentTarget.value;
    event.currentTarget.value = "";
    if (text) {
      dispatch({ type: "insert", text });
      afterTyping(text);
    }
  };

  const onCompositionStart = () => {
    composingRef.current = true;
    setComposing(true);
  };

  const onCompositionEnd = (event: CompositionEvent<HTMLTextAreaElement>) => {
    composingRef.current = false;
    setComposing(false);
    event.currentTarget.value = "";
    if (event.data) {
      dispatch({ type: "insert", text: event.data });
      afterTyping(event.data);
    }
  };

  const onPaste = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    event.preventDefault();
    const text = event.clipboardData.getData("text/plain");
    if (text) {
      dispatch({ type: "insert", text });
    }
  };

  const onClipboardEvent = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    event.preventDefault();
    sessionRef.current?.clipboard(event.type === "cut" ? "cut" : "copy");
  };

  const positionAt = (clientX: number, clientY: number): Position | null => {
    const lines = linesRef.current;
    const current = viewRef.current;
    if (!lines || !current) {
      return null;
    }
    const top = lines.getBoundingClientRect().top;
    const row = Math.floor((clientY - top) / lineHeight);
    const line = current.startLine + row;
    if (line < 0) {
      return { line: 0, column: 0 };
    }
    if (line >= current.lineCount) {
      return { line: current.lineCount - 1, column: END_OF_LINE };
    }
    const element = lines.children[line - current.startLine];
    const column = element ? columnAt(element, clientX, top + (row + 0.5) * lineHeight) : 0;
    return { line, column };
  };

  const onMouseDown = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    inputRef.current?.focus();
    hideHover();
    closeCompletion();
    const position = positionAt(event.clientX, event.clientY);
    if (!position) {
      return;
    }
    const definitionClick =
      (isMac() ? event.metaKey : event.ctrlKey) && !event.shiftKey && !event.altKey;
    if (definitionClick) {
      dispatch({ type: "setCursor", position, extend: false, add: false });
      void goToDefinition(position);
      return;
    }
    if (event.detail === 2) {
      dispatch({ type: "selectWord", position });
      return;
    }
    dispatch({ type: "setCursor", position, extend: event.shiftKey, add: event.altKey });

    const onMove = (move: MouseEvent) => {
      const to = positionAt(move.clientX, move.clientY);
      if (to) {
        dispatch({ type: "setCursor", position: to, extend: true, add: false });
      }
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  const hideHover = () => {
    window.clearTimeout(hoverTimerRef.current);
    hoverSeqRef.current++;
    if (hoverRef.current) {
      setHover(null);
    }
  };

  /** Where a popup of `height` goes: below `line`, or above it if there is no room. */
  const placeAt = (line: number, left: number, height: number): Placement | null => {
    const frame = frameRef.current;
    const lines = linesRef.current;
    const current = viewRef.current;
    if (!frame || !lines || !current) {
      return null;
    }
    const frameBox = frame.getBoundingClientRect();
    const lineTop =
      lines.getBoundingClientRect().top - frameBox.top + (line - current.startLine) * lineHeight;
    const x = Math.max(0, Math.min(left, frameBox.width - COMPLETION_WIDTH));
    const fitsBelow = lineTop + lineHeight + height <= frameBox.height;
    return fitsBelow || lineTop < height
      ? { top: lineTop + lineHeight, left: x }
      : { bottom: frameBox.height - lineTop, left: x };
  };

  const showHoverAt = async (position: Position, x: number) => {
    const seq = ++hoverSeqRef.current;
    let result: lsp.Hover | null = null;
    try {
      result = await lsp.hover(documentId, position);
    } catch (error) {
      console.debug("hover failed", error);
    }
    const frame = frameRef.current;
    if (seq !== hoverSeqRef.current || !result || !frame) {
      return;
    }
    const left = x - frame.getBoundingClientRect().left;
    // Hover cards vary in height: show below the line in the upper half.
    const placement = placeAt(position.line, left, frame.clientHeight / 2);
    if (placement) {
      setHover({ hover: result, line: position.line, x, placement });
    }
  };

  const showHoverAtCursor = async () => {
    const current = viewRef.current;
    const caret = overlay.carets.find((c) => c.primary);
    const lines = linesRef.current;
    if (current && caret && lines) {
      await showHoverAt(current.primary.position, lines.getBoundingClientRect().left + caret.left);
    }
  };

  const onContentMouseMove = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (event.buttons !== 0) {
      return;
    }
    const { clientX, clientY } = event;
    const position = positionAt(clientX, clientY);
    const shown = hoverRef.current;
    if (shown && position?.line === shown.line && Math.abs(clientX - shown.x) < HOVER_SLACK) {
      return;
    }
    hideHover();
    if (!position || !viewRef.current?.language) {
      return;
    }
    hoverTimerRef.current = window.setTimeout(() => {
      // Only over text, not the empty space after a line.
      const lines = linesRef.current;
      const element = lines?.children[position.line - (viewRef.current?.startLine ?? 0)];
      if (!lines || !element) {
        return;
      }
      const end = lines.getBoundingClientRect().left + columnX(element, lineLength(element));
      if (clientX <= end) {
        void showHoverAt(position, clientX);
      }
    }, HOVER_DELAY);
  };

  const onContentMouseLeave = (event: ReactMouseEvent<HTMLDivElement>) => {
    const to = event.relatedTarget;
    const card = frameRef.current?.querySelector(".hover-card");
    if (!(to instanceof Node && card?.contains(to))) {
      hideHover();
    }
  };

  const goToDefinition = async (position?: Position) => {
    const at = position ?? viewRef.current?.primary.position;
    if (!at) {
      return;
    }
    try {
      const location = await lsp.definition(documentId, at);
      if (location) {
        await openPath(location.path, { start: location.start });
      } else {
        notify("info", "No definition found.");
      }
    } catch (error) {
      reportError("Going to the definition", error);
    }
  };

  const closeCompletion = () => {
    completionSeqRef.current++;
    completionPendingRef.current = null;
    if (completionRef.current) {
      setCompletion(null);
    }
  };

  /**
   * Asks for completions at the cursor: right away, or (`afterEdit`) once
   * the view reflects the edit just dispatched.
   */
  const requestCompletion = (trigger: string | null, afterEdit: boolean) => {
    const current = viewRef.current;
    if (!current?.language?.completionTriggers) {
      return;
    }
    if (afterEdit) {
      completionPendingRef.current = { trigger };
    } else {
      void fetchCompletion(current.primary.position, trigger);
    }
  };

  const fetchCompletion = async (position: Position, trigger: string | null) => {
    const seq = ++completionSeqRef.current;
    let result: lsp.Completions | null = null;
    try {
      result = await lsp.completion(documentId, position, trigger);
    } catch (error) {
      console.debug("completion failed", error);
    }
    if (seq !== completionSeqRef.current) {
      return;
    }
    setCompletion(
      result && result.items.length > 0
        ? { items: result.items, incomplete: result.incomplete, selected: 0 }
        : null,
    );
  };

  /** Opens, refines, or closes completion after `text` was typed. */
  const afterTyping = (text: string) => {
    const triggers = viewRef.current?.language?.completionTriggers;
    const last = text.slice(-1);
    if (!triggers || !last) {
      return;
    }
    const open = completionRef.current;
    if (triggers.includes(last)) {
      requestCompletion(last, true);
    } else if (isWordChar(last)) {
      if (!open || open.incomplete) {
        requestCompletion(null, true);
      }
    } else {
      closeCompletion();
    }
  };

  const acceptCompletion = (item: lsp.CompletionItem) => {
    // Select from the item's start to the cursor, then replace it.
    dispatch({ type: "setCursor", position: item.from, extend: true, add: false });
    dispatch({ type: "insert", text: item.insert });
    closeCompletion();
    inputRef.current?.focus();
  };

  const primaryCaret: Rect | undefined = overlay.carets.find((c) => c.primary);
  const cursorLine = view?.primary.position.line ?? 0;
  const shownCompletions =
    completion && view
      ? visibleCompletions(
          completion.items,
          view.primary.position,
          view.lines[cursorLine - view.startLine]?.text ?? "",
        )
      : null;
  const completionLeft =
    (linesRef.current?.getBoundingClientRect().left ?? 0) -
    (frameRef.current?.getBoundingClientRect().left ?? 0) +
    (primaryCaret?.left ?? 0);
  const completionPlacement =
    shownCompletions && shownCompletions.length > 0 && primaryCaret
      ? placeAt(
          cursorLine,
          completionLeft,
          Math.min(shownCompletions.length, COMPLETION_ROWS) * COMPLETION_ROW_HEIGHT + 2,
        )
      : null;

  const startLine = view?.startLine ?? 0;
  const lineCount = view?.lineCount ?? 1;
  const primary = primaryCaret;
  const activeLine = view?.primary.position.line;
  const layerStyle = { transform: `translateY(${startLine * lineHeight}px)` };

  return (
    <div className="editor-view-frame" ref={frameRef}>
      {find && (
        <FindWidget
          state={find}
          result={view?.search ?? null}
          onQueryChange={onFindQueryChange}
          onReplacementChange={(replacement) => setFind({ ...find, replacement })}
          onToggleReplace={() => setFind({ ...find, showReplace: !find.showReplace })}
          onNext={findNext}
          onReplace={() =>
            dispatch({ type: "replace", query: find.query, replacement: find.replacement })
          }
          onReplaceAll={() =>
            dispatch({ type: "replaceAll", query: find.query, replacement: find.replacement })
          }
          onClose={closeFind}
        />
      )}
      <div className="editor-view" ref={scrollRef} onScroll={onScroll}>
        <div className="editor-view__sizer" style={{ minHeight: lineCount * lineHeight }}>
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
                  <Line key={startLine + index} line={line} />
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
                style={{ top: primary?.top ?? 0, left: primary?.left ?? 0 }}
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
  );
}

function earlier(a: Position, b: Position): Position {
  return a.line < b.line || (a.line === b.line && a.column <= b.column) ? a : b;
}

const Line = memo(function Line({ line }: { line: DisplayLine }) {
  const text = line.spans.length === 0 ? line.text : highlighted(line.text, line.spans);
  if (line.fullLength === null) {
    return <div className="editor-view__line">{text}</div>;
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
  );
});

/** `text` split into plain and highlighted runs (see `measure.ts`). */
function highlighted(text: string, spans: readonly number[]): ReactNode[] {
  const parts: ReactNode[] = [];
  let at = 0;
  for (let i = 0; i + 2 < spans.length; i += 3) {
    const start = spans[i]!;
    const end = Math.min(spans[i + 1]!, text.length);
    if (start < at || start >= end) {
      continue;
    }
    if (start > at) {
      parts.push(text.slice(at, start));
    }
    parts.push(
      <span key={start} className={`tok-${TOKEN_KINDS[spans[i + 2]! - 1] ?? "none"}`}>
        {text.slice(start, end)}
      </span>,
    );
    at = end;
  }
  if (at < text.length) {
    parts.push(text.slice(at));
  }
  return parts;
}

function visibleRange(scroller: HTMLElement, lineHeight: number): [number, number] {
  const first = Math.floor(scroller.scrollTop / lineHeight);
  const count = Math.ceil(scroller.clientHeight / lineHeight) + 1;
  return [first, first + count];
}

function viewportFor(scroller: HTMLElement | null, lineHeight: number): Viewport {
  if (!scroller) {
    return { startLine: 0, endLine: OVERSCAN * 2 };
  }
  const [first, last] = visibleRange(scroller, lineHeight);
  return { startLine: Math.max(0, first - OVERSCAN), endLine: last + OVERSCAN };
}

/** Whether `view` already contains every visible line. */
function covers(view: View | null, scroller: HTMLElement, lineHeight: number): boolean {
  if (!view) {
    return false;
  }
  const [first, last] = visibleRange(scroller, lineHeight);
  return (
    view.startLine <= first && view.startLine + view.lines.length >= Math.min(last, view.lineCount)
  );
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
    scroller.scrollTop = top;
  } else if (top + lineHeight > scroller.scrollTop + scroller.clientHeight) {
    scroller.scrollTop = top + lineHeight - scroller.clientHeight;
  }
  if (left === undefined) {
    return;
  }
  const margin = 2 * lineHeight;
  const textWidth = scroller.clientWidth - gutterWidth;
  if (left < scroller.scrollLeft) {
    scroller.scrollLeft = Math.max(0, left - margin);
  } else if (left > scroller.scrollLeft + textWidth - margin) {
    scroller.scrollLeft = left - textWidth + margin;
  }
}
