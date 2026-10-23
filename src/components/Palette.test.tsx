import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { registerCommands } from "../commands/registry";
import { activeEditor, type EditorController } from "../editor/activeEditor";
import { Palette } from "./Palette";
import { openPalette, paletteStore } from "./paletteState";

describe("Palette", () => {
  let unregister = () => {};
  const save = vi.fn();
  const disabled = vi.fn();

  beforeEach(() => {
    unregister = registerCommands([
      { id: "file.save", category: "File", title: "Save", keybinding: "mod+s", run: save },
      {
        id: "file.never",
        category: "File",
        title: "Never Shown",
        enabled: () => false,
        run: disabled,
      },
    ]);
  });

  afterEach(() => {
    unregister();
    activeEditor.set(null);
    act(() => paletteStore.set({ open: false, initial: "", session: 0 }));
    vi.clearAllMocks();
  });

  it("lists enabled commands with their keybindings and runs the chosen one", async () => {
    const user = userEvent.setup();
    render(<Palette />);
    act(() => openPalette(">"));

    expect(screen.getByRole("option", { name: /File: Save/ })).toHaveTextContent("Ctrl+S");
    expect(screen.queryByRole("option", { name: /Never Shown/ })).not.toBeInTheDocument();

    await user.keyboard("sav{Enter}");
    expect(save).toHaveBeenCalledOnce();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("goes to a line in the active editor", async () => {
    const user = userEvent.setup();
    const dispatch = vi.fn();
    const editor: EditorController = {
      documentId: 1,
      dispatch,
      openFind: vi.fn(),
      findNext: vi.fn(),
      refresh: vi.fn(),
      focus: vi.fn(),
      lineCount: () => 120,
    };
    activeEditor.set(editor);
    render(<Palette />);
    act(() => openPalette(":"));

    await user.keyboard("42");
    expect(screen.getByRole("option")).toHaveTextContent("Go to line 42of 120");
    await user.keyboard("{Enter}");
    expect(dispatch).toHaveBeenCalledWith({
      type: "setCursor",
      position: { line: 41, column: 0 },
      extend: false,
      add: false,
    });
  });

  it("closes on Escape", async () => {
    const user = userEvent.setup();
    render(<Palette />);
    act(() => openPalette(">"));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("explains that files need an open folder", () => {
    render(<Palette />);
    act(() => openPalette(""));
    expect(screen.getByText("Open a folder to search its files")).toBeInTheDocument();
  });
});
