/**
 * Typed wrappers for the document and editing commands in
 * `src-tauri/src/editor/commands.rs`. Shapes mirror `editor/protocol.rs`.
 *
 * Positions are a line plus a UTF-16 column, i.e. a JavaScript string index
 * into that line's text.
 */
import { call } from "./index";

export interface Position {
  line: number;
  column: number;
}

/** Lines `startLine..endLine` (end exclusive). */
export interface Viewport {
  startLine: number;
  endLine: number;
}

export type Motion =
  | "left"
  | "right"
  | "wordLeft"
  | "wordRight"
  | "up"
  | "down"
  | "pageUp"
  | "pageDown"
  | "lineStart"
  | "lineEnd"
  | "documentStart"
  | "documentEnd";

export type EditorOp =
  | { type: "insert"; text: string }
  | { type: "newline" }
  | { type: "tab" }
  | { type: "delete"; direction: "backward" | "forward"; unit: "grapheme" | "word" }
  | { type: "deleteSelection" }
  | { type: "move"; motion: Motion; extend: boolean; pageLines?: number }
  | { type: "selectAll" }
  | { type: "collapseSelections" }
  | { type: "setCursor"; position: Position; extend: boolean; add: boolean }
  | { type: "selectWord"; position: Position }
  | { type: "addCursor"; direction: "up" | "down" }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "findNext"; query: SearchQuery; backward: boolean }
  | { type: "selectAllMatches"; query: SearchQuery }
  | { type: "replace"; query: SearchQuery; replacement: string }
  | { type: "replaceAll"; query: SearchQuery; replacement: string }
  | { type: "addNextMatch" };

export interface SearchQuery {
  pattern: string;
  regex: boolean;
  caseSensitive: boolean;
  wholeWord: boolean;
}

export interface Range {
  start: Position;
  end: Position;
}

/** Find results included in a view when the request carried a query. */
export interface SearchView {
  /** Matches on the returned lines. */
  matches: Range[];
  total: number;
  truncated: boolean;
  /** Index of the match the primary selection covers, if any. */
  current: number | null;
  /** Why the query could not be used, e.g. an invalid regular expression. */
  error: string | null;
}

export type ClipboardAction = "copy" | "cut";

export interface DocumentSummary {
  id: number;
  title: string;
  path: string | null;
  dirty: boolean;
  lineEnding: string;
  encoding: string;
}

export interface SelectionView {
  anchor: Position;
  head: Position;
  primary: boolean;
}

export interface EditorView {
  documentId: number;
  version: number;
  lineCount: number;
  startLine: number;
  /** Text of lines `startLine..startLine + lines.length`, without line breaks. */
  lines: string[];
  /** Selections touching the returned lines, plus the primary selection. */
  selections: SelectionView[];
  selectionCount: number;
  primary: { position: Position; visualColumn: number };
  dirty: boolean;
  search: SearchView | null;
}

export const listDocuments = () => call<DocumentSummary[]>("document_list");
export const newDocument = () => call<DocumentSummary>("document_new");
export const openDocumentDialog = () => call<DocumentSummary | null>("document_open_dialog");
export const saveDocument = (id: number) => call<DocumentSummary | null>("document_save", { id });
export const saveDocumentAs = (id: number) =>
  call<DocumentSummary | null>("document_save_as", { id });
export const closeDocument = (id: number) => call<boolean>("document_close", { id });
export const openDocument = (path: string) => call<DocumentSummary>("document_open", { path });

export const getView = (id: number, viewport: Viewport, search: SearchQuery | null) =>
  call<EditorView>("editor_view", { id, viewport, search });
export const applyOps = (
  id: number,
  ops: EditorOp[],
  viewport: Viewport,
  search: SearchQuery | null,
) => call<EditorView>("editor_apply", { id, ops, viewport, search });
export const clipboard = (
  id: number,
  action: ClipboardAction,
  viewport: Viewport,
  search: SearchQuery | null,
) => call<EditorView>("editor_clipboard", { id, action, viewport, search });
