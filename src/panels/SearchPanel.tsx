import { useEffect, useState } from 'react'
import type { SearchQuery } from '../ipc/editor'
import type { LineMatch } from '../ipc/workspace'
import { openPath } from '../state/documents'
import { runSearch, searchStore, setSearchQuery, type FileResults } from '../state/search'
import { useStore } from '../state/store'
import { openFolder, workspaceStore } from '../state/workspace'
import './SearchPanel.css'

const SEARCH_DELAY_MS = 250

const toggles: { key: keyof Omit<SearchQuery, 'pattern'>; label: string; glyph: string }[] = [
  { key: 'caseSensitive', label: 'Match Case', glyph: 'Aa' },
  { key: 'wholeWord', label: 'Match Whole Word', glyph: 'ab' },
  { key: 'regex', label: 'Use Regular Expression', glyph: '.*' },
]

/** Find in files across the open workspace. */
export function SearchPanel() {
  const { info } = useStore(workspaceStore)
  const { query, running, files, summary, error } = useStore(searchStore)

  // Search as the query settles.
  useEffect(() => {
    if (!info) {
      return
    }
    const timer = setTimeout(() => void runSearch(), SEARCH_DELAY_MS)
    return () => clearTimeout(timer)
  }, [query, info])

  if (!info) {
    return (
      <div className="panel-empty">
        <p>Open a folder to search its files.</p>
        <button type="button" className="panel-button" onClick={() => void openFolder()}>
          Open Folder…
        </button>
      </div>
    )
  }

  return (
    <div className="search-panel">
      <div className="search-panel__query" data-invalid={error ? true : undefined}>
        <input
          id="search-panel-query"
          className="search-panel__input"
          aria-label="Search in files"
          placeholder="Search"
          spellCheck={false}
          value={query.pattern}
          title={error ?? undefined}
          onChange={event => setSearchQuery({ pattern: event.target.value })}
          onKeyDown={event => {
            if (event.key === 'Enter') {
              void runSearch()
            }
          }}
        />
        {toggles.map(toggle => (
          <button
            key={toggle.key}
            type="button"
            className="search-panel__option"
            aria-label={toggle.label}
            aria-pressed={query[toggle.key]}
            title={toggle.label}
            onClick={() => setSearchQuery({ [toggle.key]: !query[toggle.key] })}
          >
            {toggle.glyph}
          </button>
        ))}
      </div>
      <p className="search-panel__summary" role="status">
        {describe(query.pattern, running, files, summary, error)}
      </p>
      <ul className="search-results" aria-label="Search results">
        {files.map(file => (
          <FileResult key={file.path} file={file} />
        ))}
      </ul>
    </div>
  )
}

function FileResult({ file }: { file: FileResults }) {
  const [open, setOpen] = useState(true)
  const slash = file.relative.lastIndexOf('/')
  return (
    <li className="search-results__file">
      <button
        type="button"
        className="search-results__file-header"
        aria-expanded={open}
        title={file.path}
        onClick={() => setOpen(!open)}
      >
        <span className="search-results__chevron" aria-hidden="true">
          {open ? '▾' : '▸'}
        </span>
        <span className="search-results__name">{file.relative.slice(slash + 1)}</span>
        {slash > 0 && <span className="search-results__dir">{file.relative.slice(0, slash)}</span>}
        <span className="search-results__count">{file.matches.length}</span>
      </button>
      {open && (
        <ul className="search-results__matches">
          {file.matches.map((match, index) => (
            <MatchResult key={index} path={file.path} match={match} />
          ))}
        </ul>
      )}
    </li>
  )
}

function MatchResult({ path, match }: { path: string; match: LineMatch }) {
  const units = match.preview
  const before = units.slice(0, match.previewStart).trimStart()
  return (
    <li>
      <button
        type="button"
        className="search-results__match"
        title={`Line ${match.line + 1}`}
        onClick={() =>
          void openPath(path, {
            start: { line: match.line, column: match.startColumn },
            end: { line: match.line, column: match.endColumn },
          })
        }
      >
        {before}
        <mark>{units.slice(match.previewStart, match.previewEnd)}</mark>
        {units.slice(match.previewEnd)}
      </button>
    </li>
  )
}

function describe(
  pattern: string,
  running: boolean,
  files: readonly FileResults[],
  summary: {
    matches: number
    filesWithMatches: number
    truncated: boolean
    cancelled: boolean
  } | null,
  error: string | null,
): string {
  if (!pattern) {
    return ''
  }
  if (error) {
    return error
  }
  if (running) {
    const found = files.reduce((sum, file) => sum + file.matches.length, 0)
    return found > 0 ? `Searching… ${found} results so far` : 'Searching…'
  }
  if (!summary || summary.cancelled) {
    return ''
  }
  if (summary.matches === 0) {
    return 'No results.'
  }
  const results = `${summary.matches} result${summary.matches === 1 ? '' : 's'}`
  const inFiles = `in ${summary.filesWithMatches} file${summary.filesWithMatches === 1 ? '' : 's'}`
  return `${results} ${inFiles}${summary.truncated ? ' (search stopped at the result limit)' : ''}`
}
