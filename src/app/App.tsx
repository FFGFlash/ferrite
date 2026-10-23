import { useEffect, useState } from "react";
import { getAppInfo, isDesktop, log, notifyFrontendReady, type AppInfo } from "../ipc";
import { loadDocuments } from "../state/documents";
import { Workbench } from "./Workbench";

export function App() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);

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
      void loadDocuments();
    }
    notifyFrontendReady();
    return () => {
      cancelled = true;
    };
  }, []);

  return <Workbench appInfo={appInfo} />;
}
