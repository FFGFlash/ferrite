import { useEffect } from "react";
import { dismiss, notifications, reportError, type Notification } from "../state/notifications";
import { useStore } from "../state/store";
import "./Notifications.css";

const AUTO_DISMISS_MS = 10_000;

export function Notifications() {
  const items = useStore(notifications);
  return (
    <div className="notifications" aria-live="polite">
      {items.map((item) => (
        <NotificationItem key={item.id} item={item} />
      ))}
    </div>
  );
}

function NotificationItem({ item }: { item: Notification }) {
  const sticky = (item.actions?.length ?? 0) > 0;
  useEffect(() => {
    if (sticky) {
      return;
    }
    const timer = setTimeout(() => dismiss(item.id), AUTO_DISMISS_MS);
    return () => clearTimeout(timer);
  }, [item.id, sticky]);

  return (
    <div
      className="notification"
      data-kind={item.kind}
      role={item.kind === "error" ? "alert" : "status"}
    >
      <div className="notification__body">
        <p className="notification__message">{item.message}</p>
        {sticky && (
          <div className="notification__actions">
            {item.actions?.map((action) => (
              <button
                key={action.label}
                type="button"
                className="notification__action"
                onClick={() => {
                  dismiss(item.id);
                  Promise.resolve(action.run()).catch((error: unknown) =>
                    reportError(action.label, error),
                  );
                }}
              >
                {action.label}
              </button>
            ))}
          </div>
        )}
      </div>
      <button
        type="button"
        className="notification__dismiss"
        aria-label="Dismiss notification"
        onClick={() => dismiss(item.id)}
      >
        ×
      </button>
    </div>
  );
}
