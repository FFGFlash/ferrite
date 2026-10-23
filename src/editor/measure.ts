/**
 * DOM measurement for the editor view. Each rendered line is an element
 * whose only child is a single text node (or nothing, for an empty line), so
 * UTF-16 columns map directly to text-node offsets.
 */
import type { Position, Range, SelectionView } from "../ipc/editor";

/** Width of the selection highlight shown for a selected line break. */
const LINE_BREAK_WIDTH = 6;

export interface Rect {
  top: number;
  left: number;
  width: number;
}

export interface Overlay {
  carets: (Rect & { primary: boolean })[];
  selections: Rect[];
  /** Find matches; `current` marks the match the primary selection covers. */
  matches: (Rect & { current: boolean })[];
}

export const EMPTY_OVERLAY: Overlay = { carets: [], selections: [], matches: [] };

/** Horizontal offset of `column` from the start of a line element. */
export function columnX(line: Element, column: number): number {
  const text = line.firstChild;
  if (!text || column <= 0) {
    return 0;
  }
  const range = line.ownerDocument.createRange();
  range.setStart(text, 0);
  range.setEnd(text, Math.min(column, text.textContent?.length ?? 0));
  if (typeof range.getBoundingClientRect !== "function") {
    return 0;
  }
  return range.getBoundingClientRect().right - line.getBoundingClientRect().left;
}

/** The column under a horizontal client coordinate within a line element. */
export function columnAt(line: Element, clientX: number, clientY: number): number {
  const text = line.firstChild;
  if (!text || clientX <= line.getBoundingClientRect().left) {
    return 0;
  }
  const doc = line.ownerDocument;
  const position = doc.caretPositionFromPoint?.(clientX, clientY);
  if (position?.offsetNode === text) {
    return position.offset;
  }
  const range = doc.caretRangeFromPoint?.(clientX, clientY);
  if (range?.startContainer === text) {
    return range.startOffset;
  }
  return text.textContent?.length ?? 0;
}

/**
 * Carets and selection rectangles for the rendered lines, positioned relative
 * to the first rendered line (`startLine`).
 */
export function measureOverlay(
  lines: HTMLElement,
  startLine: number,
  selections: readonly SelectionView[],
  matches: readonly Range[],
  lineHeight: number,
): Overlay {
  const count = lines.children.length;
  const lineAt = (line: number) =>
    line >= startLine && line < startLine + count ? lines.children[line - startLine] : undefined;
  const overlay: Overlay = { carets: [], selections: [], matches: [] };

  /** Rectangles covering `start..end`, one per rendered line. */
  const rectsFor = (from: Position, to: Position): Rect[] => {
    const [start, end] = ordered(from, to);
    const rects: Rect[] = [];
    const first = Math.max(start.line, startLine);
    const last = Math.min(end.line, startLine + count - 1);
    for (let line = first; line <= last; line++) {
      const element = lineAt(line);
      if (!element) {
        continue;
      }
      const left = line === start.line ? columnX(element, start.column) : 0;
      const right =
        line === end.line
          ? columnX(element, end.column)
          : columnX(element, element.textContent?.length ?? 0) + LINE_BREAK_WIDTH;
      if (right > left) {
        rects.push({ top: (line - startLine) * lineHeight, left, width: right - left });
      }
    }
    return rects;
  };

  let primary: SelectionView | undefined;
  for (const selection of selections) {
    if (selection.primary) {
      primary = selection;
    }
    const head = lineAt(selection.head.line);
    if (head) {
      overlay.carets.push({
        top: (selection.head.line - startLine) * lineHeight,
        left: columnX(head, selection.head.column),
        width: 0,
        primary: selection.primary,
      });
    }
    overlay.selections.push(...rectsFor(selection.anchor, selection.head));
  }

  for (const match of matches) {
    const current =
      primary !== undefined &&
      samePosition(...ordered(primary.anchor, primary.head), match.start, match.end);
    overlay.matches.push(...rectsFor(match.start, match.end).map((rect) => ({ ...rect, current })));
  }
  return overlay;
}

function samePosition(a: Position, b: Position, c: Position, d: Position): boolean {
  return a.line === c.line && a.column === c.column && b.line === d.line && b.column === d.column;
}

function ordered(a: Position, b: Position): [Position, Position] {
  return a.line < b.line || (a.line === b.line && a.column <= b.column) ? [a, b] : [b, a];
}
