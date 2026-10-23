import { useEffect } from "react";
import { dismiss, notifications, type Notification } from "../state/notifications";
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
  useEffect(() => {
    const timer = setTimeout(() => dismiss(item.id), AUTO_DISMISS_MS);
    return () => clearTimeout(timer);
  }, [item.id]);

  return (
    <div
      className="notification"
      data-kind={item.kind}
      role={item.kind === "error" ? "alert" : "status"}
    >
      <p className="notification__message">{item.message}</p>
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
