import { useMemo, useRef, useState, type KeyboardEvent, type RefObject } from 'react'
import { CopyMinus, FilePlus, FolderOpen, FolderPlus } from 'lucide-react'
import { executeCommand, primaryKeybinding, getCommand } from '../commands/registry'
import type { DirectoryEntry } from '../ipc/workspace'
import { openContextMenu, type MenuEntry } from '../components/contextMenuState'
import { documentsStore, openPath } from '../state/documents'
import { iconThemeStore, type LoadedIconTheme } from '../state/iconTheme'
import { contributedMenuItems, runContributed } from '../state/extensions'
import { layoutStore } from '../state/layout'
import { reportError } from '../state/notifications'
import { useStore } from '../state/store'
import { themeKind } from '../themes/theme'
import {
  createEntry,
  openFolder,
  toggleFolder,
  workspaceStore,
  type Listing,
} from '../state/workspace'
import {
  beginCreate,
  cancelCreate,
  explorerStore,
  setFocused,
  type PendingCreate,
} from './explorerState'
import { FileIcon } from '../components/FileIcon'
import { entryIcon } from '../components/fileIcons'
import './ExplorerPanel.css'

const createEntries = (target: string): MenuEntry[] => [
  { label: 'New File…', run: () => beginCreate(false, target) },
  { label: 'New Folder…', run: () => beginCreate(true, target) },
]

/**
 * The context menu of a row. A file's menu opens it and has extensions'
 * items, which get its path.
 */
function openRowMenu(x: number, y: number, entry: DirectoryEntry): void {
  if (entry.directory) {
    openContextMenu(x, y, createEntries(entry.path))
    return
  }
  openContextMenu(x, y, [
    { label: 'Open', run: () => openPath(entry.path) },
    null,
    ...createEntries(entry.path),
    null,
    ...contributedMenuItems('explorer/context').map(item => ({
      label: item.title,
      run: () => runContributed(item.command, entry.path),
    })),
  ])
}

interface Row {
  entry: DirectoryEntry
  depth: number
  expanded: boolean
}

/** Folders and files of the open workspace, loaded as folders expand. */
export function ExplorerPanel() {
  const { info, listings } = useStore(workspaceStore)
  const { expanded } = useStore(layoutStore)
  const { documents, activeId } = useStore(documentsStore)
  const { focused, creating } = useStore(explorerStore)
  const iconTheme = useStore(iconThemeStore)
  const light = useStore(themeKind) === 'light'
  const treeRef = useRef<HTMLUListElement>(null)
  const activePath = documents.find(document => document.id === activeId)?.path ?? null

  const rows = useMemo(
    () => (info ? flatten(info.root, listings, new Set(expanded), 0) : []),
    [info, listings, expanded],
  )

  if (!info) {
    return (
      <div className="panel-empty">
        <p>No folder open.</p>
        <button type="button" className="panel-button" onClick={() => void openFolder()}>
          Open Folder…
        </button>
      </div>
    )
  }
  const rootListing = listings.get(info.root)
  if (rootListing && 'error' in rootListing) {
    return (
      <p className="panel-empty">
        Could not read {info.name}: {rootListing.error}
      </p>
    )
  }

  const focusIndex = Math.max(
    0,
    rows.findIndex(row => row.entry.path === focused),
  )
  const activate = (row: Row) => {
    setFocused(row.entry.path)
    if (row.entry.directory) {
      toggleFolder(row.entry.path)
    } else {
      void openPath(row.entry.path)
    }
  }

  // The name field goes first among the new entry's siblings.
  let inputIndex = -1
  let inputDepth = 0
  if (creating) {
    const parentIndex = rows.findIndex(row => row.entry.path === creating.parent)
    inputIndex = parentIndex + 1
    inputDepth = parentIndex >= 0 ? (rows[parentIndex]?.depth ?? 0) + 1 : 0
  }
  const nameInput = creating && (
    <NameInput
      key={`${creating.parent}:${creating.directory}`}
      pending={creating}
      depth={inputDepth}
      siblings={listings.get(creating.parent)}
      iconTheme={iconTheme}
      light={light}
      treeRef={treeRef}
    />
  )

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    const row = rows[focusIndex]
    if (!row) {
      return
    }
    const move = (index: number) => {
      const target = rows[Math.max(0, Math.min(rows.length - 1, index))]
      if (target) {
        setFocused(target.entry.path)
        document.getElementById(rowId(target.entry.path))?.scrollIntoView({ block: 'nearest' })
      }
    }
    switch (event.key) {
      case 'ArrowDown':
        move(focusIndex + 1)
        break
      case 'ArrowUp':
        move(focusIndex - 1)
        break
      case 'Home':
        move(0)
        break
      case 'End':
        move(rows.length - 1)
        break
      case 'ArrowRight':
        if (row.entry.directory && !row.expanded) {
          toggleFolder(row.entry.path)
        } else {
          move(focusIndex + 1)
        }
        break
      case 'ArrowLeft':
        if (row.entry.directory && row.expanded) {
          toggleFolder(row.entry.path)
        } else {
          const parent = rows
            .slice(0, focusIndex)
            .reverse()
            .find(candidate => candidate.depth < row.depth)
          if (parent) {
            move(rows.indexOf(parent))
          }
        }
        break
      case 'Enter':
      case ' ':
        activate(row)
        break
      default:
        return
    }
    event.preventDefault()
  }

  return (
    <div className="explorer">
      <ul
        ref={treeRef}
        className="explorer-tree"
        role="tree"
        aria-label={info.name}
        tabIndex={0}
        aria-activedescendant={rows[focusIndex] ? rowId(rows[focusIndex].entry.path) : undefined}
        onKeyDown={onKeyDown}
        onContextMenu={event => {
          // From the keyboard (Shift+F10, the menu key): the focused row's menu.
          const row = rows[focusIndex]
          if (event.target !== event.currentTarget || !row) {
            return
          }
          event.preventDefault()
          const rect = document.getElementById(rowId(row.entry.path))?.getBoundingClientRect()
          if (rect) {
            openRowMenu(rect.left + rect.width / 3, rect.bottom, row.entry)
          }
        }}
      >
        {inputIndex === 0 && nameInput}
        {rows.map((row, index) => {
          const folder = row.entry.directory ? { expanded: row.expanded } : null
          const icon = entryIcon(row.entry.name, folder, iconTheme, light)
          return [
            <li
              key={row.entry.path}
              id={rowId(row.entry.path)}
              className="explorer-tree__row"
              role="treeitem"
              aria-level={row.depth + 1}
              aria-expanded={row.entry.directory ? row.expanded : undefined}
              aria-selected={row.entry.path === activePath}
              data-focused={index === focusIndex || undefined}
              data-ignored={row.entry.ignored || undefined}
              style={indent(row.depth)}
              title={row.entry.path}
              onClick={() => activate(row)}
              onContextMenu={event => {
                event.preventDefault()
                setFocused(row.entry.path)
                openRowMenu(event.clientX, event.clientY, row.entry)
              }}
            >
              <span className="explorer-tree__chevron" aria-hidden="true">
                {row.entry.directory ? (row.expanded ? '▾' : '▸') : ''}
              </span>
              <FileIcon
                icon={icon}
                kind={folder ? 'folder' : 'file'}
                className="explorer-tree__icon"
              />
              <span className="explorer-tree__name">{row.entry.name}</span>
            </li>,
            index + 1 === inputIndex && nameInput,
          ]
        })}
        {rows.length === 0 && !creating && (
          <li className="panel-empty" role="none">
            {rootListing ? 'This folder is empty.' : 'Loading…'}
          </li>
        )}
      </ul>
      {/* The space below the rows: its menu creates at the top level. The
          header actions and commands do the same from the keyboard. */}
      <div
        className="explorer__blank"
        aria-hidden="true"
        onContextMenu={event => {
          event.preventDefault()
          openContextMenu(event.clientX, event.clientY, createEntries(info.root))
        }}
      />
    </div>
  )
}

interface NameInputProps {
  pending: PendingCreate
  depth: number
  /** The parent's listing, to warn about a name that is taken. */
  siblings: Listing | undefined
  treeRef: RefObject<HTMLUListElement | null>
  iconTheme: LoadedIconTheme | null
  light: boolean
}

/**
 * The field for naming a new file or folder. Enter creates it, keeping the
 * field open if that fails; Escape cancels. Leaving the field creates it if
 * a name was typed.
 */
function NameInput({ pending, depth, siblings, treeRef, iconTheme, light }: NameInputProps) {
  const [name, setName] = useState('')
  const [error, setError] = useState<string | null>(null)
  // Set once the field has been submitted or cancelled, so a blur as it
  // unmounts does nothing.
  const done = useRef(false)
  const kind = pending.directory ? 'folder' : 'file'

  const finish = (path: string | null) => {
    done.current = true
    explorerStore.update(state => ({
      creating: null,
      focused: path ?? state.focused,
    }))
    treeRef.current?.focus()
    if (path && !pending.directory) {
      void openPath(path)
    }
  }

  const taken = takenName(name, siblings)
  const message = error ?? (taken ? `A file or folder named ${taken} already exists here.` : null)

  const submit = async (fromBlur: boolean) => {
    if (done.current) {
      return
    }
    done.current = true
    // Leaving an empty field, or one already showing a problem, cancels.
    if (!name.trim() || (fromBlur && message)) {
      cancelCreate()
      return
    }
    try {
      finish(await createEntry(pending.parent, name, pending.directory))
    } catch (failure) {
      if (fromBlur) {
        cancelCreate()
        reportError(`Creating the ${kind}`, failure)
      } else {
        done.current = false
        setError(failure instanceof Error ? failure.message : String(failure))
      }
    }
  }
  const folder = pending.directory ? { expanded: false } : null
  const icon = entryIcon(name.trim(), folder, iconTheme, light)
  return (
    <li
      className="explorer-tree__row explorer-tree__row--input"
      role="treeitem"
      aria-level={depth + 1}
      aria-selected={false}
      aria-label={`New ${kind}`}
      style={indent(depth)}
    >
      <span className="explorer-tree__chevron" aria-hidden="true" />
      <FileIcon icon={icon} kind={kind} className="explorer-tree__icon" />
      <input
        className="explorer-tree__input"
        aria-label={`${pending.directory ? 'Folder' : 'File'} name`}
        aria-invalid={message ? true : undefined}
        aria-describedby={message ? 'explorer-name-message' : undefined}
        autoFocus
        spellCheck={false}
        autoComplete="off"
        value={name}
        onChange={event => {
          setName(event.target.value)
          setError(null)
        }}
        onKeyDown={event => {
          // Keep the tree's navigation keys away from the field.
          event.stopPropagation()
          if (event.key === 'Enter') {
            event.preventDefault()
            void submit(false)
          } else if (event.key === 'Escape') {
            event.preventDefault()
            finish(null)
          }
        }}
        onBlur={() => void submit(true)}
      />
      {message && (
        <div id="explorer-name-message" className="explorer-tree__message" role="alert">
          {message}
        </div>
      )}
    </li>
  )
}

/** The existing sibling that `name` would clash with, if it names one directly. */
function takenName(name: string, siblings: Listing | undefined): string | null {
  const trimmed = name.trim().toLowerCase()
  if (!trimmed || !siblings || 'error' in siblings || /[/\\]/.test(trimmed)) {
    return null
  }
  return siblings.find(entry => entry.name.toLowerCase() === trimmed)?.name ?? null
}

/** Explorer actions shown in the side bar header. */
export function ExplorerActions() {
  const { info } = useStore(workspaceStore)
  const collapse = getCommand('explorer.collapseAll')
  const open = getCommand('file.openFolder')
  return (
    <>
      {info && (
        <>
          <button
            type="button"
            className="sidebar__action"
            title="New File…"
            aria-label="New File"
            onClick={() => beginCreate(false)}
          >
            <FilePlus size={16} strokeWidth={1.5} aria-hidden="true" />
          </button>
          <button
            type="button"
            className="sidebar__action"
            title="New Folder…"
            aria-label="New Folder"
            onClick={() => beginCreate(true)}
          >
            <FolderPlus size={16} strokeWidth={1.5} aria-hidden="true" />
          </button>
        </>
      )}
      <button
        type="button"
        className="sidebar__action"
        title={`Open Folder…${primaryKeybinding(open) ? ` (${primaryKeybinding(open)})` : ''}`}
        aria-label="Open Folder"
        onClick={() => void executeCommand('file.openFolder')}
      >
        <FolderOpen size={16} strokeWidth={1.5} aria-hidden="true" />
      </button>
      {info && collapse && (
        <button
          type="button"
          className="sidebar__action"
          title="Collapse Folders"
          aria-label="Collapse Folders"
          onClick={() => void executeCommand('explorer.collapseAll')}
        >
          <CopyMinus size={16} strokeWidth={1.5} aria-hidden="true" />
        </button>
      )}
    </>
  )
}

function flatten(
  directory: string,
  listings: ReadonlyMap<string, Listing>,
  expanded: ReadonlySet<string>,
  depth: number,
): Row[] {
  const listing = listings.get(directory)
  if (!listing || 'error' in listing) {
    return []
  }
  return listing.flatMap(entry => {
    const isOpen = entry.directory && expanded.has(entry.path)
    const row = { entry, depth, expanded: isOpen }
    return isOpen ? [row, ...flatten(entry.path, listings, expanded, depth + 1)] : [row]
  })
}

function indent(depth: number) {
  return { paddingLeft: `calc(${depth} * var(--space-3) + var(--space-2))` }
}

function rowId(path: string): string {
  return `explorer-${encodeURIComponent(path)}`
}
