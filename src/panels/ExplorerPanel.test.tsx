import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DirectoryEntry } from "../ipc/workspace";

vi.mock("../ipc/workspace", () => ({
  listDirectory: vi.fn(),
  getWorkspace: vi.fn(),
  openWorkspaceDialog: vi.fn(),
  closeWorkspace: vi.fn(),
}));
vi.mock("../ipc/editor", () => ({ openDocument: vi.fn() }));

const ipc = await import("../ipc/workspace");
const editorIpc = await import("../ipc/editor");
const { setWorkspace } = await import("../state/workspace");
const { DEFAULT_LAYOUT, layoutStore } = await import("../state/layout");
const { ExplorerPanel } = await import("./ExplorerPanel");

const entry = (path: string, directory = false, ignored = false): DirectoryEntry => ({
  name: path.split("/").pop() ?? path,
  path,
  directory,
  ignored,
});

const listings: Record<string, DirectoryEntry[]> = {
  "/w": [entry("/w/src", true), entry("/w/target", true, true), entry("/w/README.md")],
  "/w/src": [entry("/w/src/main.rs")],
};

describe("ExplorerPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    layoutStore.set(DEFAULT_LAYOUT);
    vi.mocked(ipc.listDirectory).mockImplementation(async (path) => listings[path] ?? []);
    vi.mocked(editorIpc.openDocument).mockResolvedValue({
      id: 1,
      title: "README.md",
      path: "/w/README.md",
      dirty: false,
      lineEnding: "LF",
      encoding: "UTF-8",
    });
  });

  async function renderTree() {
    render(<ExplorerPanel />);
    await act(async () => setWorkspace({ root: "/w", name: "w" }));
    return screen.getByRole("tree", { name: "w" });
  }

  it("offers to open a folder when none is open", () => {
    render(<ExplorerPanel />);
    act(() => setWorkspace(null));
    expect(screen.getByRole("button", { name: "Open Folder…" })).toBeInTheDocument();
  });

  it("lists the root and marks ignored entries", async () => {
    const tree = await renderTree();
    const items = within(tree).getAllByRole("treeitem");
    expect(items.map((item) => item.textContent)).toEqual(["▸src", "▸target", "README.md"]);
    expect(items[1]).toHaveAttribute("data-ignored");
  });

  it("expands folders lazily and opens files", async () => {
    const user = userEvent.setup();
    const tree = await renderTree();
    await user.click(within(tree).getByText("src"));
    expect(await within(tree).findByText("main.rs")).toBeInTheDocument();
    expect(ipc.listDirectory).toHaveBeenCalledWith("/w/src");
    expect(layoutStore.get().expanded).toEqual(["/w/src"]);

    await user.click(within(tree).getByText("README.md"));
    expect(editorIpc.openDocument).toHaveBeenCalledWith("/w/README.md");
  });

  it("navigates with the keyboard", async () => {
    const user = userEvent.setup();
    const tree = await renderTree();
    tree.focus();
    await user.keyboard("{ArrowRight}");
    expect(await within(tree).findByText("main.rs")).toBeInTheDocument();
    await user.keyboard("{ArrowDown}");
    expect(within(tree).getByText("main.rs").closest("li")).toHaveAttribute("data-focused");
    await user.keyboard("{ArrowLeft}");
    expect(within(tree).getByText("src").closest("li")).toHaveAttribute("data-focused");
    await user.keyboard("{ArrowLeft}");
    expect(within(tree).queryByText("main.rs")).not.toBeInTheDocument();
  });
});
