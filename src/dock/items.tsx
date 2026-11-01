/**
 * What each kind of dock item looks like: its tab (title, icon, unsaved or
 * exited state), its content, and the buttons shown beside the tabs while
 * it's active.
 */
import { ChevronDown, FileText, Plus, SquareTerminal, Trash2 } from 'lucide-react'
import { useEffect, useRef, type ReactNode } from 'react'
import { pages, type PageDescriptor } from '../app/pages'
import { useViews, type ViewDescriptor } from '../app/views'
import { executeCommand, getCommand, primaryKeybinding } from '../commands/registry'
import { ErrorBoundary } from '../components/ErrorBoundary'
import { FileIcon } from '../components/FileIcon'
import { Icon, type IconName } from '../components/Icon'
import { entryIcon } from '../components/fileIcons'
import { Breadcrumbs } from '../editor/Breadcrumbs'
import { EditorView } from '../editor/EditorView'
import type { DocumentSummary } from '../ipc/editor'
import type { PreviewInfo } from '../ipc/extensions'
import { PreviewPane } from '../panels/PreviewPane'
import { documentsStore } from '../state/documents'
import { iconThemeStore } from '../state/iconTheme'
import { previewsStore } from '../state/previews'
import { useStore } from '../state/store'
import { openNewTerminalMenu } from '../terminal/profileMenu'
import { closeTerminal, getSession, terminalsStore, type TerminalTab } from '../terminal/terminals'
import { themeKind } from '../themes/theme'
import { documentItem, itemId, showItem } from './dock'
import { itemKind, type ItemId } from './model'

export type ResolvedItem =
  | { kind: 'document'; item: ItemId; document: DocumentSummary }
  | { kind: 'view'; item: ItemId; view: ViewDescriptor }
  | { kind: 'terminal'; item: ItemId; terminal: TerminalTab }
  | { kind: 'preview'; item: ItemId; preview: PreviewInfo }
  | { kind: 'page'; item: ItemId; page: PageDescriptor }

/** The items that exist now (open documents, known views, running terminals, open previews, pages). */
export function useResolver(): (item: ItemId) => ResolvedItem | null {
  const { documents } = useStore(documentsStore)
  const { tabs } = useStore(terminalsStore)
  const previews = useStore(previewsStore)
  const views = useViews()
  return (item: ItemId) => {
    const id = itemId(item)
    switch (itemKind(item)) {
      case 'document': {
        const document = documents.find(candidate => candidate.id === Number(id))
        return document ? { kind: 'document', item, document } : null
      }
      case 'view': {
        const view = views.find(candidate => candidate.id === id)
        return view ? { kind: 'view', item, view } : null
      }
      case 'terminal': {
        const terminal = tabs.find(candidate => candidate.id === Number(id))
        return terminal ? { kind: 'terminal', item, terminal } : null
      }
      case 'preview': {
        const preview = previews.find(candidate => candidate.id === id)
        return preview ? { kind: 'preview', item, preview } : null
      }
      case 'page': {
        const page = pages.find(candidate => candidate.id === id)
        return page ? { kind: 'page', item, page } : null
      }
      default:
        return null
    }
  }
}

export function itemTitle(resolved: ResolvedItem): string {
  switch (resolved.kind) {
    case 'document':
      return resolved.document.title
    case 'view':
      return resolved.view.title
    case 'terminal':
      return resolved.terminal.title
    case 'preview':
      return resolved.preview.title
    case 'page':
      return resolved.page.title
  }
}

/** The tab's accessible name: the title, and whether it's unsaved or exited. */
export function itemLabel(resolved: ResolvedItem): string {
  const title = itemTitle(resolved)
  if (resolved.kind === 'document' && resolved.document.dirty) {
    return `${title}, unsaved`
  }
  if (resolved.kind === 'terminal' && resolved.terminal.exited) {
    return `${title}, exited`
  }
  return title
}

export function itemTooltip(resolved: ResolvedItem): string {
  switch (resolved.kind) {
    case 'document':
      return resolved.document.path ?? resolved.document.title
    case 'view':
      return resolved.view.title
    case 'terminal':
      return resolved.terminal.description ?? resolved.terminal.title
    case 'preview': {
      const { document } = resolved.preview
      const path = documentsStore.get().documents.find(d => d.id === document)?.path
      return path ? `${resolved.preview.title} (${path})` : resolved.preview.title
    }
    case 'page':
      return resolved.page.title
  }
}

interface ItemIconProps {
  resolved: ResolvedItem
  className?: string
  size?: number
}

export function ItemIcon({ resolved, className = 'dock-tab__icon', size = 16 }: ItemIconProps) {
  const iconTheme = useStore(iconThemeStore)
  const light = useStore(themeKind) === 'light'
  switch (resolved.kind) {
    case 'document':
      return (
        <FileIcon
          className={className}
          size={size}
          kind="file"
          icon={entryIcon(resolved.document.title, null, iconTheme, light)}
        />
      )
    case 'view':
      return (
        <span className={className} aria-hidden="true">
          <Icon name={resolved.view.icon} size={size} />
        </span>
      )
    case 'terminal':
      return (
        <SquareTerminal className={className} size={size} strokeWidth={1.5} aria-hidden="true" />
      )
    case 'preview':
      return (
        <span className={className} aria-hidden="true">
          {/* The backend allows only the view icons. */}
          <Icon name={resolved.preview.icon as IconName} size={size} />
        </span>
      )
    case 'page': {
      const PageIcon = resolved.page.icon
      return <PageIcon className={className} size={size} strokeWidth={1.5} aria-hidden="true" />
    }
  }
}

/** What the active tab shows. */
export function ItemContent({ resolved }: { resolved: ResolvedItem }): ReactNode {
  switch (resolved.kind) {
    case 'document':
      return (
        <>
          <Breadcrumbs document={resolved.document} />
          <ErrorBoundary name="The editor" resetKey={resolved.document.id}>
            <EditorView
              key={resolved.document.id}
              documentId={resolved.document.id}
              title={resolved.document.title}
            />
          </ErrorBoundary>
        </>
      )
    case 'view': {
      const View = resolved.view.component
      return (
        <div className="dock-view">
          <ErrorBoundary name={`The ${resolved.view.title} view`} resetKey={resolved.view.id}>
            <View />
          </ErrorBoundary>
        </div>
      )
    }
    case 'terminal':
      return <TerminalPane id={resolved.terminal.id} />
    case 'preview':
      return (
        <div className="dock-view">
          <ErrorBoundary name={`The preview ${resolved.preview.title}`} resetKey={resolved.item}>
            <PreviewPane key={resolved.preview.id} preview={resolved.preview} />
          </ErrorBoundary>
        </div>
      )
    case 'page': {
      const Page = resolved.page.component
      return (
        <div className="dock-view">
          <ErrorBoundary name={`The ${resolved.page.title} page`} resetKey={resolved.item}>
            <Page />
          </ErrorBoundary>
        </div>
      )
    }
  }
}

/** Buttons beside the tabs for the active item. */
export function ItemActions({ resolved }: { resolved: ResolvedItem }): ReactNode {
  if (resolved.kind === 'view') {
    const Actions = resolved.view.actions
    return Actions ? <Actions /> : null
  }
  if (resolved.kind === 'preview') {
    const { document } = resolved.preview
    return document === null ? null : (
      <button
        type="button"
        className="dock-action"
        title="Show Source"
        aria-label="Show Source"
        onClick={() => showItem(documentItem(document))}
      >
        <FileText size={16} strokeWidth={1.5} aria-hidden="true" />
      </button>
    )
  }
  if (resolved.kind === 'terminal') {
    const keys = primaryKeybinding(getCommand('terminal.new'))
    return (
      <>
        <button
          type="button"
          className="dock-action"
          title={keys ? `New Terminal (${keys})` : 'New Terminal'}
          aria-label="New Terminal"
          onClick={() => void executeCommand('terminal.new')}
        >
          <Plus size={16} strokeWidth={1.5} aria-hidden="true" />
        </button>
        <button
          type="button"
          className="dock-action dock-action--narrow"
          title="New Terminal With Profile…"
          aria-label="New Terminal With Profile"
          aria-haspopup="menu"
          onClick={event => {
            const r = event.currentTarget.getBoundingClientRect()
            void openNewTerminalMenu(r.left, r.bottom)
          }}
        >
          <ChevronDown size={14} strokeWidth={1.5} aria-hidden="true" />
        </button>
        <button
          type="button"
          className="dock-action"
          title="Kill Terminal"
          aria-label="Kill Terminal"
          onClick={() => closeTerminal(resolved.terminal.id)}
        >
          <Trash2 size={16} strokeWidth={1.5} aria-hidden="true" />
        </button>
      </>
    )
  }
  return null
}

/** Shows a terminal's emulator, moving its element here. */
function TerminalPane({ id }: { id: number }) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const session = getSession(id)
    const element = ref.current
    if (!session || !element) {
      return
    }
    session.mount(element)
    const observer = new ResizeObserver(() => session.fit())
    observer.observe(element)
    return () => observer.disconnect()
  }, [id])

  return <div ref={ref} className="terminal-view" data-terminal />
}
