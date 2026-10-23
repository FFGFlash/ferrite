import { memo } from "react";
import type { ViewDescriptor } from "../app/views";
import { Icon } from "./Icon";
import "./ActivityBar.css";

interface ActivityBarProps {
  views: readonly ViewDescriptor[];
  activeViewId: string | null;
  onSelect: (id: string) => void;
}

export const ActivityBar = memo(function ActivityBar({
  views,
  activeViewId,
  onSelect,
}: ActivityBarProps) {
  return (
    <nav className="activitybar" aria-label="Views">
      {views.map((view) => (
        <button
          key={view.id}
          type="button"
          className="activitybar__item"
          aria-label={view.title}
          aria-pressed={view.id === activeViewId}
          title={view.title}
          onClick={() => onSelect(view.id)}
        >
          <Icon name={view.icon} />
        </button>
      ))}
    </nav>
  );
});
