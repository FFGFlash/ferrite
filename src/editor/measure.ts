/**
 * DOM measurement for the editor view. A rendered line's text is split
 * across text nodes (plain runs and highlight spans), optionally followed by
 * a badge for shortened lines that is not part of the text. A UTF-16 column
 * is an offset into the concatenation of those text nodes.
 */
import type { DiagnosticView, Position, Range, SelectionView } from "../ipc/editor";

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
  diagnostics: (Rect & { severity: number })[];
}

export const EMPTY_OVERLAY: Overlay = { carets: [], selections: [], matches: [], diagnostics: [] };

/** Marks elements inside a line that are not part of its text. */
const NOT_TEXT = ".editor-view__cut";

/** The text nodes of a line element, in order. */
function textNodes(line: Element): Text[] {
  const nodes: Text[] = [];
  const walker = line.ownerDocument.createTreeWalker(line, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!node.parentElement?.closest(NOT_TEXT)) {
      nodes.push(node as Text);
    }
  }
  return nodes;
}

/** The length of a line element's text, in UTF-16 units. */
export function lineLength(line: Element): number {
  return textNodes(line).reduce((length, node) => length + node.length, 0);
}

/** Horizontal offset of `column` from the start of a line element. */
export function columnX(line: Element, column: number): number {
  if (column <= 0) {
    return 0;
  }
  const nodes = textNodes(line);
  // The node containing `column`, or the end of the last one.
  let remaining = column;
  let target: Text | undefined;
  for (const node of nodes) {
    target = node;
    if (remaining <= node.length) {
      break;
    }
    remaining -= node.length;
  }
  if (!target) {
    return 0;
  }
  const range = line.ownerDocument.createRange();
  range.setStart(target, 0);
  range.setEnd(target, Math.min(remaining, target.length));
  if (typeof range.getBoundingClientRect !== "function") {
    return 0;
  }
  return range.getBoundingClientRect().right - line.getBoundingClientRect().left;
}

/** The column under a horizontal client coordinate within a line element. */
export function columnAt(line: Element, clientX: number, clientY: number): number {
  const nodes = textNodes(line);
  if (nodes.length === 0 || clientX <= line.getBoundingClientRect().left) {
    return 0;
  }
  const doc = line.ownerDocument;
  const position = doc.caretPositionFromPoint?.(clientX, clientY);
  const range = position ? null : doc.caretRangeFromPoint?.(clientX, clientY);
  const node = position?.offsetNode ?? range?.startContainer;
  const offset = position?.offset ?? range?.startOffset ?? 0;
  let column = 0;
  for (const text of nodes) {
    if (text === node) {
      return column + offset;
    }
    column += text.length;
  }
  return column;
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
  diagnostics: readonly DiagnosticView[] = [],
): Overlay {
  const count = lines.children.length;
  const lineAt = (line: number) =>
    line >= startLine && line < startLine + count ? lines.children[line - startLine] : undefined;
  const overlay: Overlay = { carets: [], selections: [], matches: [], diagnostics: [] };

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
          : columnX(element, lineLength(element)) + LINE_BREAK_WIDTH;
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

  for (const { start, end, severity } of diagnostics) {
    const rects = rectsFor(start, end);
    const element = lineAt(start.line);
    if (rects.length === 0 && element) {
      // An empty range (e.g. "expected `;`") still gets a short mark.
      const left = columnX(element, start.column);
      rects.push({ top: (start.line - startLine) * lineHeight, left, width: LINE_BREAK_WIDTH });
    }
    overlay.diagnostics.push(...rects.map((rect) => ({ ...rect, severity })));
  }
  return overlay;
}

function samePosition(a: Position, b: Position, c: Position, d: Position): boolean {
  return a.line === c.line && a.column === c.column && b.line === d.line && b.column === d.column;
}

function ordered(a: Position, b: Position): [Position, Position] {
  return a.line < b.line || (a.line === b.line && a.column <= b.column) ? [a, b] : [b, a];
}
