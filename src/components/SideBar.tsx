import type { ViewDescriptor } from "../app/views";
import "./SideBar.css";

interface SideBarProps {
  view: ViewDescriptor;
  width: number;
}

export function SideBar({ view, width }: SideBarProps) {
  const ViewComponent = view.component;
  const Actions = view.actions;
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
        <ViewComponent />
      </div>
    </aside>
  );
}
