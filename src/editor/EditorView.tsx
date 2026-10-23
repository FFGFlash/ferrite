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
} from "react";
import type {
  DisplayLine,
  EditorOp,
  EditorView as View,
  Position,
  SearchQuery,
  Viewport,
} from "../ipc/editor";
import { appearance } from "../state/appearance";
import {
  clearPendingSelection,
  pendingSelection,
  registerBeforeSave,
  setDirty,
} from "../state/documents";
import { editorStatus } from "../state/editorStatus";
import { reportError } from "../state/notifications";
import { notifyPainted, recordTiming } from "../state/perf";
import { useStore } from "../state/store";
import { activeEditor, type EditorController } from "./activeEditor";
import { FindWidget, type FindState } from "./FindWidget";
import { isMac, keyToCommand } from "./keymap";
import { columnAt, EMPTY_OVERLAY, measureOverlay, type Overlay } from "./measure";
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
  viewRef.current = view;
  findRef.current = find;
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
    });
  }, [view, documentId]);

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
    const scroller = scrollRef.current;
    if (scroller && !covers(viewRef.current, scroller, lineHeight)) {
      sessionRef.current?.setViewport(viewportFor(scroller, lineHeight));
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.nativeEvent.isComposing || composingRef.current) {
      return;
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
    const position = positionAt(event.clientX, event.clientY);
    if (!position) {
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

  const startLine = view?.startLine ?? 0;
  const lineCount = view?.lineCount ?? 1;
  const primary = overlay.carets.find((c) => c.primary);
  const activeLine = view?.primary.position.line;
  const layerStyle = { transform: `translateY(${startLine * lineHeight}px)` };

  return (
    <div className="editor-view-frame">
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
          <div className="editor-view__content" onMouseDown={onMouseDown}>
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
    </div>
  );
}

function earlier(a: Position, b: Position): Position {
  return a.line < b.line || (a.line === b.line && a.column <= b.column) ? a : b;
}

const Line = memo(function Line({ line }: { line: DisplayLine }) {
  if (line.fullLength === null) {
    return <div className="editor-view__line">{line.text}</div>;
  }
  // A very long line shortened for display; the document keeps all of it.
  return (
    <div className="editor-view__line">
      {line.text}
      <span
        className="editor-view__cut"
        title={`Line shortened for display (${line.fullLength.toLocaleString()} characters)`}
      >
        …
      </span>
    </div>
  );
});

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
