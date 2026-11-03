import { useKeybindings } from '../commands/registry'
import { ActivityBar } from '../components/ActivityBar'
import { ContextMenu } from '../components/ContextMenu'
import { DragGhost } from '../components/DragGhost'
import { ErrorBoundary } from '../components/ErrorBoundary'
import { Notifications } from '../components/Notifications'
import { Palette } from '../components/Palette'
import { PerfOverlay } from '../components/PerfOverlay'
import { StatusBar } from '../components/StatusBar'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { TrustDialog } from '../components/TrustDialog'
import { DockArea } from '../dock/DockArea'
import type { AppInfo } from '../ipc'
import './Workbench.css'

interface WorkbenchProps {
  appInfo: AppInfo | null
}

export function Workbench({ appInfo }: WorkbenchProps) {
  useKeybindings()

  return (
    <div className="workbench">
      <div className="workbench__body">
        <ActivityBar />
        <ErrorBoundary name="The workbench layout">
          <DockArea appInfo={appInfo} />
        </ErrorBoundary>
      </div>
      <ErrorBoundary name="The status bar">
        <StatusBar appInfo={appInfo} />
      </ErrorBoundary>
      <Notifications />
      <Palette />
      <TrustDialog />
      <ConfirmDialog />
      <ContextMenu />
      <DragGhost />
      <PerfOverlay />
    </div>
  )
}
