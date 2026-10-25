import { useEffect, useState } from "react";
import { runRequestedBenchmark } from "../bench/runner";
import { builtinCommands } from "../commands/builtin";
import { registerCommands } from "../commands/registry";
import { getAppInfo, isDesktop, log, notifyFrontendReady, type AppInfo } from "../ipc";
import { getSettings } from "../ipc/workspace";
import { applySettings } from "../state/appearance";
import { loadDocuments } from "../state/documents";
import { listenToBackend } from "../state/events";
import { refreshExtensions } from "../state/extensions";
import { restoreLayout } from "../state/layout";
import { reportError } from "../state/notifications";
import { loadWorkspace } from "../state/workspace";
import { Workbench } from "./Workbench";

/** Restores settings, layout, the workspace, and open documents, in that order. */
async function restoreSession(): Promise<void> {
  listenToBackend();
  try {
    applySettings(await getSettings());
  } catch (error) {
    reportError("Loading settings", error);
  }
  let activeFile: string | null = null;
  try {
    activeFile = await restoreLayout();
  } catch (error) {
    reportError("Restoring the layout", error);
  }
  await Promise.all([loadWorkspace(), loadDocuments(activeFile), refreshExtensions()]);
  await runRequestedBenchmark();
}

export function App() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);

  useEffect(() => registerCommands(builtinCommands), []);

  useEffect(() => {
    let cancelled = false;
    getAppInfo()
      .then((info) => {
        if (!cancelled) {
          setAppInfo(info);
        }
      })
      .catch((error: unknown) => log("error", `app_info failed: ${String(error)}`));
    if (isDesktop()) {
      void restoreSession();
    }
    notifyFrontendReady();
    return () => {
      cancelled = true;
    };
  }, []);

  return <Workbench appInfo={appInfo} />;
}
