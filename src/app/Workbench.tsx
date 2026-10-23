import { useCallback } from "react";
import { useKeybindings } from "../commands/registry";
import { ActivityBar } from "../components/ActivityBar";
import { Notifications } from "../components/Notifications";
import { Palette } from "../components/Palette";
import { Sash } from "../components/Sash";
import { SideBar } from "../components/SideBar";
import { StatusBar } from "../components/StatusBar";
import { EditorArea } from "../editor/EditorArea";
import type { AppInfo } from "../ipc";
import { SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH, layoutStore, updateLayout } from "../state/layout";
import { useStore } from "../state/store";
import { builtinViews } from "./views";
import "./Workbench.css";

interface WorkbenchProps {
  appInfo: AppInfo | null;
}

export function Workbench({ appInfo }: WorkbenchProps) {
  const { sidebarVisible, sidebarWidth, activeView } = useStore(layoutStore);
  useKeybindings();

  // Selecting the visible view again collapses the side bar.
  const selectView = useCallback((id: string) => {
    const layout = layoutStore.get();
    updateLayout(
      layout.sidebarVisible && layout.activeView === id
        ? { sidebarVisible: false }
        : { sidebarVisible: true, activeView: id },
    );
  }, []);
  const resize = useCallback((width: number) => updateLayout({ sidebarWidth: width }), []);

  const view = sidebarVisible
    ? (builtinViews.find((candidate) => candidate.id === activeView) ?? builtinViews[0])
    : undefined;

  return (
    <div className="workbench">
      <div className="workbench__body">
        <ActivityBar views={builtinViews} activeViewId={view?.id ?? null} onSelect={selectView} />
        {view && (
          <>
            <SideBar view={view} width={sidebarWidth} />
            <Sash
              label="Resize side bar"
              value={sidebarWidth}
              min={SIDEBAR_MIN_WIDTH}
              max={SIDEBAR_MAX_WIDTH}
              onChange={resize}
            />
          </>
        )}
        <EditorArea appInfo={appInfo} />
      </div>
      <StatusBar appInfo={appInfo} />
      <Notifications />
      <Palette />
    </div>
  );
}
