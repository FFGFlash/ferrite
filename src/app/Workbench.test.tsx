import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { AppInfo } from "../ipc";
import {
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
  Workbench,
} from "./Workbench";

const appInfo: AppInfo = {
  name: "Ferrite",
  version: "0.1.0",
  os: "windows",
  arch: "x86_64",
  tauriVersion: "2.12.5",
};

describe("Workbench", () => {
  it("opens with the explorer and the welcome tab", () => {
    render(<Workbench appInfo={null} />);

    expect(screen.getByRole("complementary", { name: "Explorer" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Explorer" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("tab", { name: "Welcome" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("heading", { name: "Ferrite" })).toBeInTheDocument();
  });

  it("switches views and collapses the side bar when the active view is selected again", async () => {
    const user = userEvent.setup();
    render(<Workbench appInfo={null} />);
    const search = screen.getByRole("button", { name: "Search" });

    await user.click(search);
    expect(screen.getByRole("complementary", { name: "Search" })).toBeInTheDocument();
    expect(search).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Explorer" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );

    await user.click(search);
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
    expect(screen.queryByRole("separator")).not.toBeInTheDocument();
    expect(search).toHaveAttribute("aria-pressed", "false");
  });

  it("resizes the side bar from the keyboard within its limits", async () => {
    const user = userEvent.setup();
    render(<Workbench appInfo={null} />);
    const sash = screen.getByRole("separator", { name: "Resize side bar" });
    const sidebar = screen.getByRole("complementary");

    sash.focus();
    await user.keyboard("{ArrowRight}");
    expect(sash).toHaveAttribute("aria-valuenow", String(SIDEBAR_DEFAULT_WIDTH + 16));

    await user.keyboard("{End}{ArrowRight}");
    expect(sash).toHaveAttribute("aria-valuenow", String(SIDEBAR_MAX_WIDTH));
    expect(sidebar).toHaveStyle({ width: `${SIDEBAR_MAX_WIDTH}px` });

    await user.keyboard("{Home}{Shift>}{ArrowLeft}{/Shift}");
    expect(sash).toHaveAttribute("aria-valuenow", String(SIDEBAR_MIN_WIDTH));
  });

  it("shows backend application details once available", () => {
    const { rerender } = render(<Workbench appInfo={null} />);
    const statusBar = screen.getByRole("contentinfo");
    expect(within(statusBar).queryByText(/Ferrite/)).not.toBeInTheDocument();

    rerender(<Workbench appInfo={appInfo} />);
    expect(within(statusBar).getByText("Ferrite 0.1.0")).toBeInTheDocument();
    expect(within(statusBar).getByText("Windows x86_64")).toBeInTheDocument();
    expect(screen.getByText("2.12.5")).toBeInTheDocument();
  });
});
