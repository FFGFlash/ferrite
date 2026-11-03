import { useState } from 'react'
import { useViews } from '../app/views'
import { activityBarItems, dockInSidebar, dockStore, toggleItem } from '../dock/dock'
import { startItemDrag } from '../dock/DockArea'
import {
  ItemIcon,
  itemLabel,
  itemTitle,
  itemTooltip,
  useResolver,
  type ResolvedItem,
} from '../dock/items'
import { groups } from '../dock/model'
import { extensionsStore } from '../state/extensions'
import { useStore } from '../state/store'
import { sectionsOf, viewBadges } from '../state/views'
import { useDropTarget } from './drag'
import './ActivityBar.css'

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
  const badges = useStore(viewBadges)
  const extensions = useStore(extensionsStore)
  /** A view's badge, or the sum of a container's sections'. */
  const badgeOf = (resolved: ResolvedItem) => {
    if (resolved.kind !== 'view') {
      return 0
    }
    const sections = sectionsOf(resolved.view.id, extensions)
    return (sections.length > 0 ? sections : [resolved.view.id]).reduce(
      (sum, view) => sum + (badges[view] ?? 0),
      0,
    )
  }
  const [dropping, setDropping] = useState(false)
  const dropRef = useDropTarget<HTMLElement>({
    over: drag => {
      const takes = drag.payload.kind === 'dock'
      setDropping(takes)
      return takes
    },
    drop: drag => {
      if (drag.payload.kind === 'dock') {
        dockInSidebar(drag.payload.item)
      }
    },
    leave: () => setDropping(false),
  })

  const items = activityBarItems(
    layout,
    views.map(view => view.id),
  )
    .map(resolve)
    .filter((resolved): resolved is ResolvedItem => resolved !== null)
  // An item counts as shown when it's the visible tab of its group.
  const shown = new Set(groups(layout).flatMap(group => (group.hidden ? [] : [group.active])))

  return (
    <nav ref={dropRef} className="activitybar" aria-label="Views" data-drop={dropping || undefined}>
      {items.map(resolved => {
        const badge = badgeOf(resolved)
        return (
          <button
            key={resolved.item}
            type="button"
            className="activitybar__item"
            aria-label={badge > 0 ? `${itemLabel(resolved)} (${badge})` : itemLabel(resolved)}
            aria-pressed={shown.has(resolved.item)}
            title={itemTooltip(resolved)}
            data-kind={resolved.kind}
            onPointerDown={event => startItemDrag(event, resolved.item, itemTitle(resolved))}
            onClick={() => toggleItem(resolved.item)}
          >
            <ItemIcon resolved={resolved} className="activitybar__icon" size={24} />
            {badge > 0 && (
              <span className="activitybar__badge" aria-hidden="true">
                {badge > 999 ? '999+' : badge}
              </span>
            )}
          </button>
        )
      })}
    </nav>
  )
}
