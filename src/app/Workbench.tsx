import { useCallback, useState } from "react";
import { ActivityBar } from "../components/ActivityBar";
import { Sash } from "../components/Sash";
import { SideBar } from "../components/SideBar";
import { StatusBar } from "../components/StatusBar";
import { EditorArea } from "../editor/EditorArea";
import type { AppInfo } from "../ipc";
import { builtinViews } from "./views";
import "./Workbench.css";

export const SIDEBAR_DEFAULT_WIDTH = 260;
export const SIDEBAR_MIN_WIDTH = 170;
export const SIDEBAR_MAX_WIDTH = 600;

interface WorkbenchProps {
  appInfo: AppInfo | null;
}

export function Workbench({ appInfo }: WorkbenchProps) {
  const [activeViewId, setActiveViewId] = useState<string | null>(builtinViews[0]?.id ?? null);
  const [sidebarWidth, setSidebarWidth] = useState(SIDEBAR_DEFAULT_WIDTH);

  // Selecting the active view again collapses the side bar.
  const selectView = useCallback((id: string) => {
    setActiveViewId((current) => (current === id ? null : id));
  }, []);

  const activeView = builtinViews.find((view) => view.id === activeViewId);

  return (
    <div className="workbench">
      <div className="workbench__body">
        <ActivityBar views={builtinViews} activeViewId={activeViewId} onSelect={selectView} />
        {activeView && (
          <>
            <SideBar view={activeView} width={sidebarWidth} />
            <Sash
              label="Resize side bar"
              value={sidebarWidth}
              min={SIDEBAR_MIN_WIDTH}
              max={SIDEBAR_MAX_WIDTH}
              onChange={setSidebarWidth}
            />
          </>
        )}
        <EditorArea appInfo={appInfo} />
      </div>
      <StatusBar appInfo={appInfo} />
    </div>
  );
}
