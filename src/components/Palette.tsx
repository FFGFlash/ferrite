import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
import { fuzzyFilter } from '../commands/fuzzy'
import { commandLabel, executeCommand, primaryKeybinding, useCommands } from '../commands/registry'
import { activeEditor, selectRange } from '../editor/activeEditor'
import { findFiles, type FileResult } from '../ipc/workspace'
import { openPath } from '../state/documents'
import { iconThemeStore } from '../state/iconTheme'
import { useStore } from '../state/store'
import { workspaceStore } from '../state/workspace'
import { themeKind } from '../themes/theme'
import { entryIcon, type EntryIcon } from './fileIcons'
import { FileIcon } from './FileIcon'
import { closePalette, paletteStore } from './paletteState'
import './Palette.css'

interface Item {
  key: string
  label: ReactNode
  detail?: ReactNode
  hint?: string | null
  /** Shown before the label (files). */
  icon?: EntryIcon
  run: () => void
}

const MAX_ITEMS = 100

/**
 * The command palette. With a `>` prefix it lists commands, with `:` it goes
 * to a line, and otherwise it finds workspace files by fuzzy path.
 */
export function Palette() {
  const { open, initial, session } = useStore(paletteStore)
  if (!open) {
    return null
  }
  return <PaletteDialog key={session} initial={initial} />
}

function PaletteDialog({ initial }: { initial: string }) {
  const [value, setValue] = useState(initial)
  const [selected, setSelected] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLUListElement>(null)
  const returnFocus = useRef<Element | null>(document.activeElement)
  const items = usePaletteItems(value)

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  useEffect(() => setSelected(0), [value])

  useEffect(() => {
    listRef.current?.children[selected]?.scrollIntoView({ block: 'nearest' })
  }, [selected])

  const dismiss = (restoreFocus: boolean) => {
    closePalette()
    if (restoreFocus && returnFocus.current instanceof HTMLElement) {
      returnFocus.current.focus()
    }
  }

  const choose = (item: Item | undefined) => {
    if (!item) {
      return
    }
    dismiss(true)
    item.run()
  }

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault()
        setSelected(index => Math.min(items.length - 1, index + 1))
        break
      case 'ArrowUp':
        event.preventDefault()
        setSelected(index => Math.max(0, index - 1))
        break
      case 'Enter':
        event.preventDefault()
        choose(items[selected])
        break
      case 'Escape':
        event.preventDefault()
        dismiss(true)
        break
    }
  }

  const activeId = items[selected] ? `palette-item-${selected}` : undefined
  return (
    <div className="palette-backdrop" onMouseDown={() => dismiss(true)}>
      <div
        className="palette"
        role="dialog"
        aria-label="Command palette"
        onMouseDown={event => event.stopPropagation()}
      >
        <input
          ref={inputRef}
          className="palette__input"
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          aria-activedescendant={activeId}
          aria-label="Type a file name, > for commands, or : for a line"
          placeholder="Search files by name (> for commands, : to go to a line)"
          value={value}
          spellCheck={false}
          onChange={event => setValue(event.target.value)}
          onKeyDown={onKeyDown}
          onBlur={() => dismiss(false)}
        />
        <ul className="palette__list" id="palette-list" role="listbox" ref={listRef}>
          {items.map((item, index) => (
            <li
              key={item.key}
              id={`palette-item-${index}`}
              className="palette__item"
              role="option"
              aria-selected={index === selected}
              onMouseMove={() => setSelected(index)}
              onMouseDown={event => {
                event.preventDefault()
                choose(item)
              }}
            >
              {item.icon && <FileIcon icon={item.icon} kind="file" className="palette__icon" />}
              <span className="palette__label">{item.label}</span>
              {item.detail && <span className="palette__detail">{item.detail}</span>}
              {item.hint && <kbd className="palette__hint">{item.hint}</kbd>}
            </li>
          ))}
          {items.length === 0 && <li className="palette__empty">{emptyMessage(value)}</li>}
        </ul>
      </div>
    </div>
  )
}

function emptyMessage(value: string): string {
  if (value.startsWith('>')) {
    return 'No matching commands'
  }
  if (value.startsWith(':')) {
    return activeEditor.get() ? 'Type a line number' : 'Open a file to go to a line'
  }
  return workspaceStore.get().info ? 'No matching files' : 'Open a folder to search its files'
}

function usePaletteItems(value: string): Item[] {
  const commands = useCommands()
  const files = useFileResults(value.startsWith('>') || value.startsWith(':') ? null : value)
  const iconTheme = useStore(iconThemeStore)
  const light = useStore(themeKind) === 'light'

  return useMemo(() => {
    if (value.startsWith('>')) {
      const visible = commands.filter(
        command => !command.hidden && (!command.enabled || command.enabled()),
      )
      return fuzzyFilter(visible, value.slice(1), commandLabel)
        .slice(0, MAX_ITEMS)
        .map(({ item: command, match }) => ({
          key: command.id,
          label: highlight(commandLabel(command), match.indices),
          hint: primaryKeybinding(command),
          run: () => void executeCommand(command.id),
        }))
    }
    if (value.startsWith(':')) {
      return goToLineItems(value.slice(1))
    }
    return files.map(file => {
      const slash = file.relative.lastIndexOf('/')
      const name = file.relative.slice(slash + 1)
      return {
        key: file.path,
        icon: entryIcon(name, null, iconTheme, light),
        label: highlight(name, shift(file.indices, slash + 1)),
        detail: slash > 0 ? highlight(file.relative.slice(0, slash), file.indices) : null,
        run: () => void openPath(file.path),
      }
    })
  }, [value, commands, files, iconTheme, light])
}

function goToLineItems(text: string): Item[] {
  const editor = activeEditor.get()
  const match = /^\s*(\d+)(?:[:,]\s*(\d+))?\s*$/.exec(text)
  if (!editor || !match) {
    return []
  }
  const line = Math.min(Number(match[1]), editor.lineCount())
  const column = match[2] ? Number(match[2]) : 1
  return [
    {
      key: 'line',
      label: `Go to line ${line}${match[2] ? `, column ${column}` : ''}`,
      detail: `of ${editor.lineCount()}`,
      run: () =>
        selectRange(editor, { line: Math.max(0, line - 1), column: Math.max(0, column - 1) }),
    },
  ]
}

/** Workspace files matching `query`, fetched from the backend as it changes. */
function useFileResults(query: string | null): FileResult[] {
  const [results, setResults] = useState<FileResult[]>([])
  const { info } = useStore(workspaceStore)
  useEffect(() => {
    if (query === null || !info) {
      setResults([])
      return
    }
    let stale = false
    const timer = setTimeout(() => {
      findFiles(query)
        .then(found => !stale && setResults(found.files))
        .catch(() => !stale && setResults([]))
    }, 30)
    return () => {
      stale = true
      clearTimeout(timer)
    }
  }, [query, info])
  return results
}

function shift(indices: readonly number[], by: number): number[] {
  return indices.map(index => index - by).filter(index => index >= 0)
}

/** `text` with the characters at `indices` emphasized. */
function highlight(text: string, indices: readonly number[]): ReactNode {
  if (indices.length === 0) {
    return text
  }
  const marked = new Set(indices)
  const parts: ReactNode[] = []
  let run = ''
  let runMarked = false
  const flush = (key: number) => {
    if (run) {
      parts.push(runMarked ? <mark key={key}>{run}</mark> : run)
    }
    run = ''
  }
  for (let index = 0; index < text.length; index++) {
    const isMarked = marked.has(index)
    if (isMarked !== runMarked) {
      flush(index)
      runMarked = isMarked
    }
    run += text[index]
  }
  flush(text.length)
  return parts
}
