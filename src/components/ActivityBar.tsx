import { useState, type DragEvent } from 'react'
import { useViews } from '../app/views'
import { activityBarItems, dockInSidebar, dockStore, toggleItem } from '../dock/dock'
import { DOCK_ITEM_TYPE, draggedItem, endDrag, startDrag } from '../dock/DockArea'
import { ItemIcon, itemLabel, itemTooltip, useResolver, type ResolvedItem } from '../dock/items'
import { groups } from '../dock/model'
import { useStore } from '../state/store'
import './ActivityBar.css'

const isDockDrag = (event: DragEvent) => event.dataTransfer.types.includes(DOCK_ITEM_TYPE)

/**
 * A button per item in the side bar (views, terminals, documents), then per
 * view not docked anywhere: shows it, or hides its group when it's already
 * shown. A button can be dragged into the dock to move the item there (and
 * out of the side bar), and a tab dropped here moves its item into the side
 * bar.
 */
export function ActivityBar() {
  const layout = useStore(dockStore)
  const views = useViews()
  const resolve = useResolver()
  const [dropping, setDropping] = useState(false)

  const items = activityBarItems(
    layout,
    views.map(view => view.id),
  )
    .map(resolve)
    .filter((resolved): resolved is ResolvedItem => resolved !== null)
  // An item counts as shown when it's the visible tab of its group.
  const shown = new Set(groups(layout).flatMap(group => (group.hidden ? [] : [group.active])))

  return (
    <nav
      className="activitybar"
      aria-label="Views"
      data-drop={dropping || undefined}
      onDragOver={event => {
        if (isDockDrag(event)) {
          event.preventDefault()
          event.dataTransfer.dropEffect = 'move'
          setDropping(true)
        }
      }}
      onDragLeave={event => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          setDropping(false)
        }
      }}
      onDrop={event => {
        setDropping(false)
        if (!isDockDrag(event)) {
          return
        }
        event.preventDefault()
        const item = event.dataTransfer.getData(DOCK_ITEM_TYPE) || draggedItem()
        endDrag()
        if (item) {
          dockInSidebar(item)
        }
      }}
    >
      {items.map(resolved => (
        <button
          key={resolved.item}
          type="button"
          className="activitybar__item"
          aria-label={itemLabel(resolved)}
          aria-pressed={shown.has(resolved.item)}
          title={itemTooltip(resolved)}
          data-kind={resolved.kind}
          draggable
          onDragStart={event => startDrag(event, resolved.item)}
          onDragEnd={() => {
            setDropping(false)
            endDrag()
          }}
          onClick={() => toggleItem(resolved.item)}
        >
          <ItemIcon resolved={resolved} className="activitybar__icon" size={24} />
        </button>
      ))}
    </nav>
  )
}
