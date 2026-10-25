import type { ReactNode } from 'react'
import type { ViewDescriptor } from '../app/views'
import { ErrorBoundary } from './ErrorBoundary'
import './SideBar.css'

interface SideBarProps {
  view: ViewDescriptor
  width: number
  /** The splitter on its edge, inside the landmark so it can be found. */
  resizer?: ReactNode
}

export function SideBar({ view, width, resizer }: SideBarProps) {
  const ViewComponent = view.component
  const Actions = view.actions
  return (
    <aside className="sidebar" style={{ width }} aria-label={view.title}>
      <header className="sidebar__header">
        <span className="sidebar__title">{view.title}</span>
        {Actions && (
          <div className="sidebar__actions">
            <Actions />
          </div>
        )}
      </header>
      <div className="sidebar__content">
        <ErrorBoundary name={`The ${view.title} view`} resetKey={view.id}>
          <ViewComponent />
        </ErrorBoundary>
      </div>
      {resizer}
    </aside>
  )
}
