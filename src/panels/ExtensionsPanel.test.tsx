import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ExtensionStatus } from "../ipc/extensions";

vi.mock("../ipc/extensions", () => ({
  restartExtension: vi.fn(() => Promise.resolve()),
  setExtensionEnabled: vi.fn(() => Promise.resolve()),
  setExtensionPermissions: vi.fn(() => Promise.resolve()),
}));

const ipc = await import("../ipc/extensions");
const { extensionsStore } = await import("../state/extensions");
const { ExtensionsPanel } = await import("./ExtensionsPanel");

const tidy: ExtensionStatus = {
  id: "example.tidy",
  name: "Tidy",
  version: "0.1.0",
  description: "Flags trailing whitespace.",
  path: "/ext/tidy",
  development: true,
  state: "active",
  error: null,
  permissions: [
    {
      permission: "documents-read",
      description: "Read open documents",
      granted: true,
      decided: true,
    },
    {
      permission: "documents-write",
      description: "Edit open documents",
      granted: false,
      decided: true,
    },
  ],
  commands: [{ id: "example.tidy.trim", title: "Trim", category: "Tidy" }],
  configErrors: ["width must be at least 0; using the default"],
};

describe("ExtensionsPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("explains how to add extensions when there are none", () => {
    extensionsStore.set([]);
    render(<ExtensionsPanel />);
    expect(screen.getByText("No extensions are installed.")).toBeInTheDocument();
  });

  it("shows each extension's state, permissions, commands, and problems", () => {
    const broken: ExtensionStatus = {
      ...tidy,
      id: "/ext/broken",
      name: "broken",
      version: "",
      state: "invalid",
      error: "requires extension API version 2",
      permissions: [],
      commands: [],
      configErrors: [],
      development: false,
    };
    extensionsStore.set([tidy, broken]);
    render(<ExtensionsPanel />);
    expect(screen.getByText("Tidy")).toBeInTheDocument();
    expect(screen.getByText("Active")).toBeInTheDocument();
    expect(screen.getByText("dev")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Read open documents" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Edit open documents" })).not.toBeChecked();
    expect(screen.getByRole("button", { name: "Trim" })).toBeInTheDocument();
    expect(screen.getByText(/width must be at least 0/)).toBeInTheDocument();
    expect(screen.getByText("Invalid")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("requires extension API version 2");
  });

  it("changes permissions and turns extensions off", async () => {
    const user = userEvent.setup();
    extensionsStore.set([tidy]);
    render(<ExtensionsPanel />);
    await user.click(screen.getByRole("checkbox", { name: "Edit open documents" }));
    expect(ipc.setExtensionPermissions).toHaveBeenCalledWith("example.tidy", [
      "documents-read",
      "documents-write",
    ]);
    await user.click(screen.getByRole("button", { name: "Disable" }));
    expect(ipc.setExtensionEnabled).toHaveBeenCalledWith("example.tidy", false);
    await user.click(screen.getByRole("button", { name: "Restart" }));
    expect(ipc.restartExtension).toHaveBeenCalledWith("example.tidy");
  });
});
