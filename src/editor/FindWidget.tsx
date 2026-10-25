import { useEffect, useRef, type KeyboardEvent } from 'react'
import type { SearchQuery, SearchView } from '../ipc/editor'
import './FindWidget.css'

export interface FindState {
  query: SearchQuery
  replacement: string
  showReplace: boolean
  /** Changes whenever the widget should take focus (each Ctrl+F). */
  focusRequest: number
}

interface FindWidgetProps {
  state: FindState
  result: SearchView | null
  onQueryChange: (change: Partial<SearchQuery>) => void
  onReplacementChange: (replacement: string) => void
  onToggleReplace: () => void
  onNext: (backward: boolean) => void
  onReplace: () => void
  onReplaceAll: () => void
  onClose: () => void
}

const options = [
  { key: 'caseSensitive', label: 'Match Case', glyph: 'Aa', shortcut: 'c' },
  { key: 'wholeWord', label: 'Match Whole Word', glyph: 'ab', shortcut: 'w' },
  { key: 'regex', label: 'Use Regular Expression', glyph: '.*', shortcut: 'r' },
] as const

/** Find and replace for the active editor, docked at its top right. */
export function FindWidget({
  state,
  result,
  onQueryChange,
  onReplacementChange,
  onToggleReplace,
  onNext,
  onReplace,
  onReplaceAll,
  onClose,
}: FindWidgetProps) {
  const findRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    findRef.current?.focus()
    findRef.current?.select()
  }, [state.focusRequest])

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>, field: 'find' | 'replace') => {
    const option = options.find(o => event.altKey && event.key.toLowerCase() === o.shortcut)
    if (option) {
      event.preventDefault()
      onQueryChange({ [option.key]: !state.query[option.key] })
    } else if (event.key === 'Escape') {
      event.preventDefault()
      onClose()
    } else if (event.key === 'Enter') {
      event.preventDefault()
      if (field === 'find') {
        onNext(event.shiftKey)
      } else if (event.ctrlKey || event.metaKey) {
        onReplaceAll()
      } else {
        onReplace()
      }
    }
  }

  const status = !state.query.pattern
    ? ''
    : result?.error
      ? 'Invalid pattern'
      : result && result.total === null
        ? 'Counting…'
        : !result || result.total === 0
          ? 'No results'
          : `${result.current === null ? '?' : result.current + 1} of ${result.total}${result.truncated ? '+' : ''}`

  return (
    <div className="find-widget" role="search" aria-label="Find in editor">
      <button
        type="button"
        className="find-widget__toggle"
        aria-label={state.showReplace ? 'Hide replace' : 'Show replace'}
        aria-expanded={state.showReplace}
        onClick={onToggleReplace}
      >
        {state.showReplace ? '▾' : '▸'}
      </button>
      <div className="find-widget__rows">
        <div className="find-widget__row">
          <div className="find-widget__field" data-invalid={result?.error ? true : undefined}>
            <input
              ref={findRef}
              className="find-widget__input"
              aria-label="Find"
              placeholder="Find"
              title={result?.error ?? undefined}
              value={state.query.pattern}
              spellCheck={false}
              onChange={event => onQueryChange({ pattern: event.target.value })}
              onKeyDown={event => onKeyDown(event, 'find')}
            />
            {options.map(option => (
              <button
                key={option.key}
                type="button"
                className="find-widget__option"
                aria-label={option.label}
                aria-pressed={state.query[option.key]}
                title={`${option.label} (Alt+${option.shortcut.toUpperCase()})`}
                onClick={() => onQueryChange({ [option.key]: !state.query[option.key] })}
              >
                {option.glyph}
              </button>
            ))}
          </div>
          <span className="find-widget__status" role="status">
            {status}
          </span>
          <button
            type="button"
            className="find-widget__button"
            aria-label="Previous match"
            title="Previous match (Shift+Enter)"
            onClick={() => onNext(true)}
          >
            ↑
          </button>
          <button
            type="button"
            className="find-widget__button"
            aria-label="Next match"
            title="Next match (Enter)"
            onClick={() => onNext(false)}
          >
            ↓
          </button>
          <button
            type="button"
            className="find-widget__button"
            aria-label="Close find"
            title="Close (Escape)"
            onClick={onClose}
          >
            ×
          </button>
        </div>
        {state.showReplace && (
          <div className="find-widget__row">
            <div className="find-widget__field">
              <input
                className="find-widget__input"
                aria-label="Replace"
                placeholder="Replace"
                value={state.replacement}
                spellCheck={false}
                onChange={event => onReplacementChange(event.target.value)}
                onKeyDown={event => onKeyDown(event, 'replace')}
              />
            </div>
            <button
              type="button"
              className="find-widget__button find-widget__button--text"
              title="Replace (Enter)"
              onClick={onReplace}
            >
              Replace
            </button>
            <button
              type="button"
              className="find-widget__button find-widget__button--text"
              title="Replace All (Ctrl+Enter)"
              onClick={onReplaceAll}
            >
              All
            </button>
          </div>
        )}
      </div>
    </div>
  )
}
