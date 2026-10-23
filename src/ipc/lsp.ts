/**
 * Typed wrappers for the language server commands in
 * `src-tauri/src/lsp/commands.rs`. Each resolves to `null` when there is
 * nothing to show (no server, no answer, or the server failed).
 */
import { call } from "./index";
import type { Position } from "./editor";

export interface DiagnosticMessage {
  severity: number;
  message: string;
  source: string | null;
}

export interface Hover {
  markdown: string | null;
  diagnostics: DiagnosticMessage[];
}

export interface Location {
  path: string;
  start: Position;
  end: Position;
}

export interface CompletionItem {
  label: string;
  kind: string | null;
  detail: string | null;
  /** Replaces the text from `from` to the cursor. */
  insert: string;
  from: Position;
  /** Matched against what is typed after `from`. */
  filter: string;
}

export interface Completions {
  items: CompletionItem[];
  /** Typing more should ask again rather than filter these. */
  incomplete: boolean;
}

export const hover = (id: number, position: Position) =>
  call<Hover | null>("lsp_hover", { id, position });
export const definition = (id: number, position: Position) =>
  call<Location | null>("lsp_definition", { id, position });
export const completion = (id: number, position: Position, trigger: string | null) =>
  call<Completions | null>("lsp_completion", { id, position, trigger });
