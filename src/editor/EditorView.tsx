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
import type { EditorOp, EditorView as View, Position, Viewport } from "../ipc/editor";
import { registerBeforeSave, setDirty } from "../state/documents";
import { editorStatus } from "../state/editorStatus";
import { reportError } from "../state/notifications";
import { isMac, keyToCommand } from "./keymap";
import { columnAt, EMPTY_OVERLAY, measureOverlay, type Overlay } from "./measure";
import { EditorSession, ipcTransport, type EditorTransport } from "./session";
import "./EditorView.css";

/** Extra lines fetched above and below the visible area. */
const OVERSCAN = 20;
const FALLBACK_LINE_HEIGHT = 20;
/** A column beyond any line end; the backend clamps it to the line end. */
const END_OF_LINE = Number.MAX_SAFE_INTEGER;

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
  const composingRef = useRef(false);
  const revealRef = useRef(false);
  const frameRef = useRef(0);

  const [view, setView] = useState<View | null>(null);
  const [lineHeight, setLineHeight] = useState(FALLBACK_LINE_HEIGHT);
  const [overlay, setOverlay] = useState<Overlay>(EMPTY_OVERLAY);
  const [composing, setComposing] = useState(false);
  viewRef.current = view;

  useEffect(() => {
    const scroller = scrollRef.current;
    const height = readLineHeight(scroller);
    setLineHeight(height);
    const session = new EditorSession(
      documentId,
      viewportFor(scroller, height),
      setView,
      (error) => reportError("Editing", error),
      transport,
    );
    sessionRef.current = session;
    const unregister = registerBeforeSave(documentId, () => session.idle());
    inputRef.current?.focus();
    return () => {
      unregister();
      session.dispose();
      sessionRef.current = null;
      cancelAnimationFrame(frameRef.current);
      editorStatus.set(null);
    };
  }, [documentId, transport]);

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
    if (!view || !lines) {
      return;
    }
    const measured = measureOverlay(lines, view.startLine, view.selections, lineHeight);
    setOverlay(measured);
    if (revealRef.current) {
      revealRef.current = false;
      const caret = measured.carets.find((c) => c.primary);
      reveal(
        scrollRef.current,
        view.primary.position.line * lineHeight,
        lineHeight,
        caret?.left,
        gutterRef.current?.offsetWidth ?? 0,
      );
    }
  }, [view, lineHeight]);

  const dispatch = (op: EditorOp) => {
    revealRef.current = true;
    sessionRef.current?.dispatch(op);
  };

  const onScroll = () => {
    if (frameRef.current) {
      return;
    }
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = 0;
      const scroller = scrollRef.current;
      const current = viewRef.current;
      if (scroller && !covers(current, scroller, lineHeight)) {
        sessionRef.current?.setViewport(viewportFor(scroller, lineHeight));
      }
    });
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.nativeEvent.isComposing || composingRef.current) {
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
              {overlay.selections.map((rect, index) => (
                <div
                  key={index}
                  className="editor-view__selection"
                  style={{ top: rect.top, left: rect.left, width: rect.width }}
                />
              ))}
            </div>
            <div className="editor-view__lines" ref={linesRef}>
              {view?.lines.map((text, index) => (
                <Line key={startLine + index} text={text} />
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
  );
}

const Line = memo(function Line({ text }: { text: string }) {
  return <div className="editor-view__line">{text}</div>;
});

function readLineHeight(element: HTMLElement | null): number {
  if (!element) {
    return FALLBACK_LINE_HEIGHT;
  }
  const value = parseFloat(getComputedStyle(element).getPropertyValue("--editor-line-height"));
  return Number.isFinite(value) && value > 0 ? value : FALLBACK_LINE_HEIGHT;
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
  scroller: HTMLElement | null,
  top: number,
  lineHeight: number,
  left: number | undefined,
  gutterWidth: number,
): void {
  if (!scroller) {
    return;
  }
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
