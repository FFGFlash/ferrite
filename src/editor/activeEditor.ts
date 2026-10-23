/**
 * A handle on the editor view of the active document, so commands (find,
 * go to line, add next match) can act on it without knowing about React.
 */
import type { EditorOp, Position } from "../ipc/editor";
import { createStore } from "../state/store";

export interface EditorController {
  documentId: number;
  dispatch(op: EditorOp): void;
  /** Opens the find widget, prefilled from a single-line selection. */
  openFind(replace: boolean): void;
  /** Moves to the next or previous match of the find widget's query. */
  findNext(backward: boolean): void;
  /** Re-fetches the view, e.g. after the document was reloaded from disk. */
  refresh(): void;
  focus(): void;
  lineCount(): number;
  /** Scrolls so `line` is at the top of the viewport. */
  scrollToLine(line: number): void;
}

export const activeEditor = createStore<EditorController | null>(null);

/** Selects `start..end` (or places the cursor at `start`) and scrolls to it. */
export function selectRange(editor: EditorController, start: Position, end = start): void {
  editor.dispatch({ type: "setCursor", position: start, extend: false, add: false });
  if (end.line !== start.line || end.column !== start.column) {
    editor.dispatch({ type: "setCursor", position: end, extend: true, add: false });
  }
  editor.focus();
}
