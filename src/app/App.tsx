import { useEffect, useState } from "react";
import { getAppInfo, log, notifyFrontendReady, type AppInfo } from "../ipc";
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
    notifyFrontendReady();
    return () => {
      cancelled = true;
    };
  }, []);

  return <Workbench appInfo={appInfo} />;
}
