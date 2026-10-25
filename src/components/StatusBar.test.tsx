import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { registerCommands } from "../commands/registry";
import { statusItemsStore } from "../state/extensions";
import { StatusBar } from "./StatusBar";

describe("StatusBar", () => {
  it("shows extension items by side and priority, and runs their commands", async () => {
    const user = userEvent.setup();
    const run = vi.fn();
    const unregister = registerCommands([{ id: "example.todos.show", title: "Show", run }]);
    statusItemsStore.set([
      {
        id: "a.low",
        extension: "A",
        alignment: "right",
        priority: 1,
        text: "low",
        tooltip: null,
        command: null,
      },
      {
        id: "a.high",
        extension: "A",
        alignment: "right",
        priority: 9,
        text: "high",
        tooltip: "Tip",
        command: "example.todos.show",
      },
      {
        id: "a.left",
        extension: "A",
        alignment: "left",
        priority: 0,
        text: "left",
        tooltip: null,
        command: null,
      },
    ]);
    render(<StatusBar appInfo={null} />);
    const items = screen.getAllByText(/^(low|high|left)$/).map((element) => element.textContent);
    expect(items).toEqual(["left", "high", "low"]);
    await user.click(screen.getByRole("button", { name: "high" }));
    expect(run).toHaveBeenCalled();
    expect(screen.getByText("low")).toHaveAttribute("title", "A");
    unregister();
    statusItemsStore.set([]);
  });
});
