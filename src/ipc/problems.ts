/** Typed wrapper for the file problems command (`src-tauri/src/problems.rs`). */
import type { Position } from './editor'
import { call } from './index'

/** A diagnostic in a file, from a language server or an extension. */
export interface FileProblem {
  /** UTF-16 columns, as the editor's (in a file that isn't open, as its server gave them). */
  start: Position
  end: Position
  /** 1 error, 2 warning, 3 information. */
  severity: 1 | 2 | 3
  message: string
  /** The language server or extension that reported it. */
  source: string | null
}

export interface FileProblems {
  /** `null` for an untitled document. */
  path: string | null
  /** The open document's id; `null` for a file that isn't open. */
  document: number | null
  title: string
  /** Most severe first, then in order. */
  problems: FileProblem[]
}

export interface ProblemsList {
  /** Files with problems, by path; untitled documents last. */
  files: FileProblems[]
  /** Problems beyond the most listed. */
  leftOut: number
}

/** Problems in open documents, and those language servers reported for other files. */
export const listFileProblems = () => call<ProblemsList>('problems_list')
