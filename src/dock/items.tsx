/**
 * What each kind of dock item looks like: its tab (title, icon, unsaved or
 * exited state), its content, and the buttons shown beside the tabs while
 * it's active.
 */
import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  Columns2,
  FileText,
  FoldVertical,
  Pilcrow,
  Plus,
  SquarePlus,
  SquareTerminal,
  Trash2,
  Undo2,
} from 'lucide-react'
import { useEffect, useRef, type ReactNode } from 'react'
import { findPage, type PageDescriptor } from '../app/pages'
import { useViews, type ViewDescriptor } from '../app/views'
import { executeCommand, getCommand, primaryKeybinding } from '../commands/registry'
import { ErrorBoundary } from '../components/ErrorBoundary'
import { FileIcon } from '../components/FileIcon'
import { Icon, isActionIcon, type IconName } from '../components/Icon'
import { entryIcon } from '../components/fileIcons'
import { Breadcrumbs } from '../editor/Breadcrumbs'
import { DiffView } from '../editor/DiffView'
import { MergeView } from '../editor/MergeView'
import { EditorView } from '../editor/EditorView'
import type { DocumentSummary } from '../ipc/editor'
import type { MergeSpec } from '../ipc/extensions'
import {
  runExtensionCommand,
  type ExtensionStatus,
  type PreviewInfo,
  type TabAction,
} from '../ipc/extensions'
import { PreviewPane } from '../panels/PreviewPane'
import { ViewerPane } from '../panels/ViewerPane'
import {
  applyDiffRanges,
  diffControls,
  diffsStore,
  isAddress,
  loadDiff,
  revertDiffRanges,
  parseDiffItem,
  type DiffSpec,
  type DiffState,
} from '../state/diffs'
import { documentsStore, openPath, openProvided } from '../state/documents'
import { loadMerge, mergesStore, parseMergeItem, type MergeState } from '../state/merges'
import { layoutStore, updateLayout } from '../state/layout'
import { extensionsStore, usableExtensions } from '../state/extensions'
import { iconThemeStore } from '../state/iconTheme'
import { reportError } from '../state/notifications'
import { previewsStore } from '../state/previews'
import { useStore } from '../state/store'
import { viewerIds } from '../state/viewers'
import { postToWebview, webviewActionStates } from '../state/webviews'
import { openNewTerminalMenu } from '../terminal/profileMenu'
import { closeTerminal, getSession, terminalsStore, type TerminalTab } from '../terminal/terminals'
import { themeKind } from '../themes/theme'
import { documentTab, itemId, parseDocumentItem, parseViewerItem, showItem } from './dock'
import { itemKind, type ItemId } from './model'

export type ResolvedItem =
  /** A document's tab, showing its view `view`. */
  | { kind: 'document'; item: ItemId; document: DocumentSummary; view: number }
  | { kind: 'view'; item: ItemId; view: ViewDescriptor }
  | { kind: 'terminal'; item: ItemId; terminal: TerminalTab }
  | { kind: 'preview'; item: ItemId; preview: PreviewInfo }
  | { kind: 'page'; item: ItemId; page: PageDescriptor }
  /** A file in an extension's viewer, or (`viewer: null`) a binary file to open as text. */
  | { kind: 'viewer'; item: ItemId; viewer: string | null; path: string }
  /** Two documents compared; `state` is undefined until they start opening. */
  | { kind: 'diff'; item: ItemId; spec: DiffSpec; state: DiffState | undefined }
  /** A merge's versions above its result; `state` is undefined until they start opening. */
  | { kind: 'merge'; item: ItemId; spec: MergeSpec; state: MergeState | undefined }

const fileName = (path: string) => path.split(/[/\\]/).pop() || path

/** The items that exist now (open documents, known views, running terminals, open previews, pages). */
export function useResolver(): (item: ItemId) => ResolvedItem | null {
  const { documents } = useStore(documentsStore)
  const { tabs } = useStore(terminalsStore)
  const previews = useStore(previewsStore)
  const extensions = useStore(extensionsStore)
  const views = useViews()
  const diffs = useStore(diffsStore)
  const merges = useStore(mergesStore)
  return (item: ItemId) => {
    const id = itemId(item)
    switch (itemKind(item)) {
      case 'document': {
        const { document: documentId, view } = parseDocumentItem(item)
        const document = documents.find(candidate => candidate.id === documentId)
        return document ? { kind: 'document', item, document, view } : null
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
        const page = findPage(id, extensions)
        return page ? { kind: 'page', item, page } : null
      }
      case 'viewer':
        return { kind: 'viewer', item, ...parseViewerItem(item) }
      case 'diff': {
        const spec = parseDiffItem(item)
        return spec ? { kind: 'diff', item, spec, state: diffs[item] } : null
      }
      case 'merge': {
        const spec = parseMergeItem(item)
        return spec ? { kind: 'merge', item, spec, state: merges[item] } : null
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
    case 'viewer':
      return fileName(resolved.path)
    case 'diff':
    case 'merge':
      return resolved.spec.title
  }
}

/** The file a tab shows, for its explorer marks: a document's, a viewer's, or a diff's modified file. */
export function itemPath(resolved: ResolvedItem): string | null {
  switch (resolved.kind) {
    case 'document':
      return resolved.document.path
    case 'viewer':
      return resolved.path
    case 'diff':
      return isAddress(resolved.spec.modified) ? null : resolved.spec.modified
    case 'merge':
      return resolved.spec.result
    default:
      return null
  }
}

/** Whether a tab has unsaved changes. */
export function itemDirty(resolved: ResolvedItem): boolean {
  return (
    (resolved.kind === 'document' && resolved.document.dirty) ||
    ((resolved.kind === 'diff' || resolved.kind === 'merge') &&
      resolved.state?.status === 'ready' &&
      resolved.state.dirty)
  )
}

/** The tab's accessible name: the title, and whether it's unsaved or exited. */
export function itemLabel(resolved: ResolvedItem): string {
  const title = itemTitle(resolved)
  if (itemDirty(resolved)) {
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
    case 'viewer':
      return resolved.path
    case 'diff':
      return `${resolved.spec.original} ↔ ${resolved.spec.modified}`
    case 'merge':
      return resolved.spec.result
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
    case 'viewer':
      return (
        <FileIcon
          className={className}
          size={size}
          kind="file"
          icon={entryIcon(fileName(resolved.path), null, iconTheme, light)}
        />
      )
    case 'diff':
      return (
        <FileIcon
          className={className}
          size={size}
          kind="file"
          icon={entryIcon(sideName(resolved.spec.modified), null, iconTheme, light)}
        />
      )
    case 'merge':
      return (
        <FileIcon
          className={className}
          size={size}
          kind="file"
          icon={entryIcon(sideName(resolved.spec.result), null, iconTheme, light)}
        />
      )
  }
}

/** The file name at the end of a diff side's path or address. */
const sideName = (side: string) => side.split(/[/\\:]/).pop() || side

/** What the active tab shows. */
export function ItemContent({ resolved }: { resolved: ResolvedItem }): ReactNode {
  switch (resolved.kind) {
    case 'document':
      return (
        <>
          <Breadcrumbs document={resolved.document} />
          <ErrorBoundary name="The editor" resetKey={resolved.item}>
            <EditorView
              key={resolved.item}
              documentId={resolved.document.id}
              view={resolved.view}
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
    case 'diff':
      return (
        <ErrorBoundary name={`The diff ${resolved.spec.title}`} resetKey={resolved.item}>
          <DiffPane key={resolved.item} item={resolved.item} state={resolved.state} />
        </ErrorBoundary>
      )
    case 'merge':
      return (
        <ErrorBoundary name={`The merge ${resolved.spec.title}`} resetKey={resolved.item}>
          <MergePane
            key={resolved.item}
            item={resolved.item}
            spec={resolved.spec}
            state={resolved.state}
          />
        </ErrorBoundary>
      )
    case 'viewer':
      return (
        <div className="dock-view">
          <ErrorBoundary name={`The viewer of ${fileName(resolved.path)}`} resetKey={resolved.item}>
            <ViewerPane
              key={resolved.item}
              item={resolved.item}
              viewer={resolved.viewer}
              path={resolved.path}
            />
          </ErrorBoundary>
        </div>
      )
  }
}

/**
 * Whether an item stays loaded while its tab is hidden: a webview whose
 * extension asks for it (`retain_when_hidden`). Other items are unmounted
 * and drawn again when shown.
 */
export function keepsLoaded(
  resolved: ResolvedItem,
  extensions: readonly ExtensionStatus[],
): boolean {
  const contributions = extensions.map(extension => extension.contributions)
  switch (resolved.kind) {
    case 'viewer':
      return contributions.some(contribution =>
        contribution.viewers.some(
          viewer => viewer.fullId === resolved.viewer && viewer.webview?.retainWhenHidden,
        ),
      )
    case 'preview': {
      // `<extension id>.<preview id>/<key>`.
      const declared = resolved.preview.id.split('/')[0]
      return (
        resolved.preview.webview &&
        contributions.some(contribution =>
          contribution.previews.some(
            preview => preview.fullId === declared && preview.webview?.retainWhenHidden,
          ),
        )
      )
    }
    default:
      return false
  }
}

/** Buttons beside the tabs for the active item. */
export function ItemActions({ resolved }: { resolved: ResolvedItem }): ReactNode {
  if (resolved.kind === 'view') {
    const Actions = resolved.view.actions
    return Actions ? <Actions /> : null
  }
  if (resolved.kind === 'viewer' && resolved.viewer !== null) {
    return <ExtensionTabActions resolved={resolved} />
  }
  if (resolved.kind === 'preview') {
    const { document } = resolved.preview
    return (
      <>
        <ExtensionTabActions resolved={resolved} />
        {document !== null && (
          <button
            type="button"
            className="dock-action"
            title="Show Source"
            aria-label="Show Source"
            onClick={() => showItem(documentTab(document))}
          >
            <FileText size={16} strokeWidth={1.5} aria-hidden="true" />
          </button>
        )}
      </>
    )
  }
  if (resolved.kind === 'diff') {
    return <DiffActions resolved={resolved} />
  }
  if (resolved.kind === 'merge') {
    const { result } = resolved.spec
    return (
      <button
        type="button"
        className="dock-action"
        title="Open File"
        aria-label="Open File"
        onClick={() => void openPath(result)}
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

/**
 * The buttons an extension declares for a viewer's or preview's tabs. A
 * click runs the action's command, with the tab as its argument (the
 * viewer's file, or the preview's key), or goes to the webview's page,
 * which can set each button's state.
 */
function ExtensionTabActions({
  resolved,
}: {
  resolved: Extract<ResolvedItem, { kind: 'viewer' | 'preview' }>
}): ReactNode {
  const extensions = useStore(extensionsStore)
  const ids = useStore(viewerIds)
  const states = useStore(webviewActionStates)
  // `<extension id>.<preview id>/<key>`, or `<viewer id>/<path>` once opened.
  const id = resolved.kind === 'preview' ? resolved.preview.id : ids[resolved.item]
  const declared = resolved.kind === 'preview' ? resolved.preview.id.split('/')[0] : resolved.viewer
  const actions = extensions
    .flatMap<{ fullId: string; actions: TabAction[] }>(extension =>
      resolved.kind === 'preview'
        ? extension.contributions.previews
        : extension.contributions.viewers,
    )
    .find(contribution => contribution.fullId === declared)?.actions
  if (!actions || actions.length === 0) {
    return null
  }
  const argument =
    resolved.kind === 'preview'
      ? resolved.preview.id.slice(resolved.preview.id.indexOf('/') + 1)
      : resolved.path
  const run = (action: TabAction) => {
    if (action.command) {
      runExtensionCommand(action.command, null, argument).catch((error: unknown) =>
        reportError(action.title, error),
      )
    } else if (id) {
      postToWebview(id, 'action', action.id)
    }
  }
  return actions.map(action => {
    const state = (id && states[id]?.[action.id]) || {}
    if (state.hidden) {
      return null
    }
    const title = state.title ?? action.title
    return (
      <button
        key={action.id}
        type="button"
        className="dock-action"
        title={title}
        aria-label={title}
        aria-pressed={state.pressed}
        disabled={state.disabled || (!action.command && !id)}
        onClick={() => run(action)}
      >
        {action.icon.kind === 'named' && isActionIcon(action.icon.name) ? (
          <Icon name={action.icon.name} size={16} />
        ) : action.icon.kind === 'image' ? (
          <img className="dock-action__image" src={action.icon.url} alt="" />
        ) : null}
      </button>
    )
  })
}

/** A merge tab: opens its documents, then shows the merge. */
function MergePane({
  item,
  spec,
  state,
}: {
  item: ItemId
  spec: MergeSpec
  state: MergeState | undefined
}) {
  useEffect(() => {
    void loadMerge(item)
  }, [item])
  if (state?.status === 'ready') {
    return <MergeView item={item} spec={spec} state={state} />
  }
  return (
    <div className="dock-view diff-view-status" role="status">
      {state?.status === 'failed' ? `Couldn't open the merge: ${state.message}` : 'Opening…'}
    </div>
  )
}

/** A diff tab: opens its documents, then compares them. */
function DiffPane({ item, state }: { item: ItemId; state: DiffState | undefined }) {
  useEffect(() => {
    void loadDiff(item)
  }, [item])
  if (state?.status === 'ready') {
    return <DiffView item={item} original={state.original} modified={state.modified} />
  }
  return (
    <div className="dock-view diff-view-status" role="status">
      {state?.status === 'failed' ? `Couldn't open the diff: ${state.message}` : 'Opening…'}
    </div>
  )
}

/** A diff tab's buttons: moving between changes, how changes show, and the file. */
function DiffActions({ resolved }: { resolved: Extract<ResolvedItem, { kind: 'diff' }> }) {
  const { diff } = useStore(layoutStore)
  const toggle = (key: keyof typeof diff) => updateLayout({ diff: { ...diff, [key]: !diff[key] } })
  const button = (
    label: string,
    command: string | null,
    icon: ReactNode,
    onClick: () => void,
    pressed?: boolean,
  ) => {
    const keys = command ? primaryKeybinding(getCommand(command)) : null
    return (
      <button
        type="button"
        className="dock-action"
        title={keys ? `${label} (${keys})` : label}
        aria-label={label}
        aria-pressed={pressed}
        onClick={onClick}
      >
        {icon}
      </button>
    )
  }
  const { original, modified } = resolved.spec
  const extensions = useStore(extensionsStore)
  // What the extension providing the original calls applying changes to it.
  const scheme = isAddress(original) ? original.slice(0, original.indexOf(':')) : null
  const applyTitle =
    scheme === null
      ? null
      : (usableExtensions(extensions)
          .map(extension => extension.contributions)
          .find(contributions =>
            contributions.textProviders.some(provider => provider.scheme === scheme),
          )?.quickDiff?.applyTitle ?? null)
  const editable = !isAddress(modified)
  return (
    <>
      {editable &&
        applyTitle &&
        button(
          `${applyTitle} Selected Ranges`,
          null,
          <SquarePlus size={16} strokeWidth={1.5} aria-hidden="true" />,
          () => void applyDiffRanges(resolved.item),
        )}
      {editable &&
        button(
          'Revert Selected Ranges',
          'diff.revertSelectedRanges',
          <Undo2 size={16} strokeWidth={1.5} aria-hidden="true" />,
          () => void revertDiffRanges(resolved.item),
        )}
      {button(
        'Previous Change',
        'diff.previousChange',
        <ArrowUp size={16} strokeWidth={1.5} aria-hidden="true" />,
        () => diffControls(resolved.item)?.goToChange(true),
      )}
      {button(
        'Next Change',
        'diff.nextChange',
        <ArrowDown size={16} strokeWidth={1.5} aria-hidden="true" />,
        () => diffControls(resolved.item)?.goToChange(false),
      )}
      {button(
        'Side by Side',
        'diff.toggleInline',
        <Columns2 size={16} strokeWidth={1.5} aria-hidden="true" />,
        () => toggle('inline'),
        !diff.inline,
      )}
      {button(
        'Show Leading and Trailing Whitespace Differences',
        'diff.toggleWhitespace',
        <Pilcrow size={16} strokeWidth={1.5} aria-hidden="true" />,
        () => toggle('ignoreTrimWhitespace'),
        !diff.ignoreTrimWhitespace,
      )}
      {button(
        'Collapse Unchanged Lines',
        'diff.toggleUnchanged',
        <FoldVertical size={16} strokeWidth={1.5} aria-hidden="true" />,
        () => toggle('hideUnchanged'),
        diff.hideUnchanged,
      )}
      {button(
        'Open File',
        null,
        <FileText size={16} strokeWidth={1.5} aria-hidden="true" />,
        () => void (isAddress(modified) ? openProvided(modified) : openPath(modified)),
      )}
    </>
  )
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
