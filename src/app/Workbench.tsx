import { useMemo } from 'react'
import { useKeybindings } from '../commands/registry'
import { ActivityBar } from '../components/ActivityBar'
import { ContextMenu } from '../components/ContextMenu'
import { ErrorBoundary } from '../components/ErrorBoundary'
import { Notifications } from '../components/Notifications'
import { Palette } from '../components/Palette'
import { PerfOverlay } from '../components/PerfOverlay'
import { StatusBar } from '../components/StatusBar'
import { toggleView, dockStore, viewItem } from '../dock/dock'
import { DockArea } from '../dock/DockArea'
import { groups } from '../dock/model'
import type { AppInfo } from '../ipc'
import { useStore } from '../state/store'
import { useViews } from './views'
import './Workbench.css'

interface WorkbenchProps {
  appInfo: AppInfo | null
}

export function Workbench({ appInfo }: WorkbenchProps) {
  const views = useViews()
  const layout = useStore(dockStore)
  useKeybindings()

  // A view counts as shown when it's the visible tab of its group.
  const shownViews = useMemo(() => {
    const shown = new Set<string>()
    for (const group of groups(layout)) {
      if (!group.hidden && group.active) {
        shown.add(group.active)
      }
    }
    return new Set(views.filter(view => shown.has(viewItem(view.id))).map(view => view.id))
  }, [layout, views])

  return (
    <div className="workbench">
      <div className="workbench__body">
        <ActivityBar views={views} shownViews={shownViews} onSelect={toggleView} />
        <ErrorBoundary name="The workbench layout">
          <DockArea appInfo={appInfo} />
        </ErrorBoundary>
      </div>
      <ErrorBoundary name="The status bar">
        <StatusBar appInfo={appInfo} />
      </ErrorBoundary>
      <Notifications />
      <Palette />
      <ContextMenu />
      <PerfOverlay />
    </div>
  )
}
