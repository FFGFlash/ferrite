import { memo } from 'react'
import type { ViewDescriptor } from '../app/views'
import { viewItem } from '../dock/dock'
import { endDrag, startDrag } from '../dock/DockArea'
import { Icon } from './Icon'
import './ActivityBar.css'

interface ActivityBarProps {
  views: readonly ViewDescriptor[]
  /** Views currently shown: their buttons are pressed. */
  shownViews: ReadonlySet<string>
  onSelect: (id: string) => void
}

/**
 * A button per view: shows it, or hides its group when it's already shown.
 * A button can also be dragged into the dock to open the view there.
 */
export const ActivityBar = memo(function ActivityBar({
  views,
  shownViews,
  onSelect,
}: ActivityBarProps) {
  return (
    <nav className="activitybar" aria-label="Views">
      {views.map(view => (
        <button
          key={view.id}
          type="button"
          className="activitybar__item"
          aria-label={view.title}
          aria-pressed={shownViews.has(view.id)}
          title={view.title}
          draggable
          onDragStart={event => startDrag(event, viewItem(view.id))}
          onDragEnd={endDrag}
          onClick={() => onSelect(view.id)}
        >
          <Icon name={view.icon} />
        </button>
      ))}
    </nav>
  )
})
