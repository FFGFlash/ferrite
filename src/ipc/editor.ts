/**
 * Typed wrappers for the document and editing commands in
 * `src-tauri/src/editor/commands.rs`. Shapes mirror `editor/protocol.rs`.
 *
 * Positions are a line plus a UTF-16 column, i.e. a JavaScript string index
 * into that line's text.
 */
import { call } from './index'

export interface Position {
  line: number
  column: number
}

/** Lines `startLine..endLine` (end exclusive). */
export interface Viewport {
  startLine: number
  endLine: number
}

export type Motion =
  | 'left'
  | 'right'
  | 'wordLeft'
  | 'wordRight'
  | 'up'
  | 'down'
  | 'pageUp'
  | 'pageDown'
  | 'lineStart'
  | 'lineEnd'
  | 'documentStart'
  | 'documentEnd'

export type EditorOp =
  | { type: 'insert'; text: string }
  | { type: 'newline' }
  | { type: 'tab' }
  | { type: 'delete'; direction: 'backward' | 'forward'; unit: 'grapheme' | 'word' }
  | { type: 'deleteSelection' }
  | { type: 'move'; motion: Motion; extend: boolean; pageLines?: number }
  | { type: 'selectAll' }
  | { type: 'collapseSelections' }
  | { type: 'setCursor'; position: Position; extend: boolean; add: boolean }
  | { type: 'selectWord'; position: Position }
  | { type: 'addCursor'; direction: 'up' | 'down' }
  | { type: 'undo' }
  | { type: 'redo' }
  | { type: 'findNext'; query: SearchQuery; backward: boolean }
  | { type: 'selectAllMatches'; query: SearchQuery }
  | { type: 'replace'; query: SearchQuery; replacement: string }
  | { type: 'replaceAll'; query: SearchQuery; replacement: string }
  | { type: 'addNextMatch' }

export interface SearchQuery {
  pattern: string
  regex: boolean
  caseSensitive: boolean
  wholeWord: boolean
}

export interface Range {
  start: Position
  end: Position
}

/** Find results included in a view when the request carried a query. */
export interface SearchView {
  /** Matches on the returned lines. */
  matches: Range[]
  /** Total matches, or `null` while a background count is running. */
  total: number | null
  truncated: boolean
  /** Index of the match the primary selection covers, if any. */
  current: number | null
  /** Why the query could not be used, e.g. an invalid regular expression. */
  error: string | null
}

export type ClipboardAction = 'copy' | 'cut' | 'paste'

export interface DocumentSummary {
  id: number
  title: string
  path: string | null
  dirty: boolean
  lineEnding: string
  encoding: string
}

export interface SelectionView {
  anchor: Position
  head: Position
  primary: boolean
}

/** Lines `start..start + removed` were replaced by `start..start + inserted`. */
export interface LineSplice {
  start: number
  removed: number
  inserted: number
}

/** A line as sent: its text, or its first part plus full length if it was cut. */
export type WireLine = string | { text: string; fullLength: number }

/**
 * How to update held lines: replay `splices` on the lines held at version
 * `base` (or drop them all when `base` is null), then fill in `chunks`.
 */
export interface LinesUpdate {
  base: number | null
  splices: LineSplice[]
  chunks: { start: number; lines: WireLine[] }[]
}

/** Lines a client holds: `startLine..endLine` at `version`. */
export interface KnownLines {
  version: number
  startLine: number
  endLine: number
}

/** Highlight categories, in protocol order: span kind `k` is `TOKEN_KINDS[k - 1]`. */
export const TOKEN_KINDS = [
  'comment',
  'keyword',
  'string',
  'escape',
  'number',
  'constant',
  'function',
  'macro',
  'type',
  'variable',
  'property',
  'operator',
  'punctuation',
  'attribute',
  'tag',
  'label',
  'module',
] as const

/**
 * A line's syntax highlights, flattened as `[start, end, kind, ...]` with
 * UTF-16 columns, in order and non-overlapping.
 */
export interface LineHighlights {
  line: number
  spans: number[]
}

/** A diagnostic on the view's lines. Severity: 1 error, 2 warning, 3 info, 4 hint. */
export interface DiagnosticView {
  start: Position
  end: Position
  severity: number
}

export interface ServerStatus {
  name: string
  state: 'starting' | 'ready' | 'busy' | 'failed'
  message: string | null
}

/** The document's language and language server. */
export interface LanguageStatus {
  language: string
  server: ServerStatus | null
  errors: number
  warnings: number
  /** Characters that open completion; `null` without completion support. */
  completionTriggers: string[] | null
}

interface ViewFields {
  documentId: number
  version: number
  lineCount: number
  /** The viewport: lines `startLine..endLine`. */
  startLine: number
  endLine: number
  /** Selections touching the viewport, plus the primary selection. */
  selections: SelectionView[]
  selectionCount: number
  primary: { position: Position; visualColumn: number }
  dirty: boolean
  search: SearchView | null
  diagnostics: DiagnosticView[]
  language: LanguageStatus | null
  /** Extension decorations on the view's lines. */
  decorations: DecorationView[]
}

/** A range decorated by an extension. */
export interface DecorationView {
  start: Position
  end: Position
  /** The decoration type: `<extension id>/<name>`. */
  key: string
}

/** A view as the backend sends it: only the lines the client was missing. */
export interface WireView extends ViewFields {
  lines: LinesUpdate
  /**
   * Highlights for lines whose spans changed. Lines in `lines.chunks` start
   * with none; other held lines keep theirs.
   */
  highlights: LineHighlights[]
}

export interface DisplayLine {
  text: string
  /** Full length in UTF-16 units when `text` was shortened for display. */
  fullLength: number | null
  /** Syntax highlights, as in `LineHighlights.spans`. */
  spans: readonly number[]
}

/** A view with every viewport line resolved, ready to render. */
export interface EditorView extends ViewFields {
  /** Lines `startLine..endLine`. */
  lines: DisplayLine[]
}

/**
 * Lines `startLine..startLine + lines.length` drawn small for the minimap:
 * each line's runs of non-blank columns as `[start, end, kind, ...]`, in
 * visual columns. Kind 0 is plain text; others index `TOKEN_KINDS` from 1.
 */
export interface MinimapView {
  documentId: number
  version: number
  lineCount: number
  startLine: number
  lines: number[][]
}

export const listDocuments = () => call<DocumentSummary[]>('document_list')
/** Creates an untitled document, optionally with `text`. */
export const newDocument = (text?: string) =>
  call<DocumentSummary>('document_new', { text: text ?? null })
export const openDocumentDialog = () => call<DocumentSummary | null>('document_open_dialog')
export const saveDocument = (id: number) => call<DocumentSummary | null>('document_save', { id })
export const saveDocumentAs = (id: number) =>
  call<DocumentSummary | null>('document_save_as', { id })
export const closeDocument = (id: number) => call<boolean>('document_close', { id })
export const openDocument = (path: string) => call<DocumentSummary>('document_open', { path })

export const getView = (
  id: number,
  viewport: Viewport,
  known: KnownLines | null,
  search: SearchQuery | null,
) => call<WireView>('editor_view', { id, viewport, known, search })
export const getMinimap = (id: number, startLine: number, endLine: number) =>
  call<MinimapView>('editor_minimap', { id, startLine, endLine })
export const applyOps = (
  id: number,
  ops: EditorOp[],
  viewport: Viewport,
  known: KnownLines | null,
  search: SearchQuery | null,
) => call<WireView>('editor_apply', { id, ops, viewport, known, search })
export const clipboard = (
  id: number,
  action: ClipboardAction,
  viewport: Viewport,
  known: KnownLines | null,
  search: SearchQuery | null,
) => call<WireView>('editor_clipboard', { id, action, viewport, known, search })
