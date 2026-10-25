import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { notifications, notify } from "../state/notifications";
import { Notifications } from "./Notifications";

describe("Notifications", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    notifications.set([]);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("hides information after a while, but not while it is hovered", () => {
    render(<Notifications />);
    act(() => {
      notify("info", "Settings applied.");
    });
    const item = screen.getByRole("status");
    fireEvent.mouseEnter(item);
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(screen.getByText("Settings applied.")).toBeInTheDocument();

    fireEvent.mouseLeave(item);
    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(screen.queryByText("Settings applied.")).toBeNull();
  });

  it("keeps warnings and errors until they are dismissed", () => {
    render(<Notifications />);
    act(() => {
      notify("warning", "Unsaved changes were restored.");
      notify("error", "Saving failed.");
    });
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(screen.getByText("Unsaved changes were restored.")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("Saving failed.");

    fireEvent.click(screen.getAllByRole("button", { name: "Dismiss notification" })[0]!);
    expect(screen.queryByText("Unsaved changes were restored.")).toBeNull();
  });
});
