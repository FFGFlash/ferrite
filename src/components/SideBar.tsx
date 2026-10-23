import type { ViewDescriptor } from "../app/views";
import "./SideBar.css";

interface SideBarProps {
  view: ViewDescriptor;
  width: number;
}

export function SideBar({ view, width }: SideBarProps) {
  const ViewComponent = view.component;
  return (
    <aside className="sidebar" style={{ width }} aria-label={view.title}>
      <header className="sidebar__header">{view.title}</header>
      <div className="sidebar__content">
        <ViewComponent />
      </div>
    </aside>
  );
}
