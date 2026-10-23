import { describe, expect, it } from "vitest";
import { keyToCommand, type KeyInput } from "./keymap";

const key = (k: string, modifiers: Partial<KeyInput> = {}): KeyInput => ({
  key: k,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...modifiers,
});

const win = (input: KeyInput) => keyToCommand(input, false, 30);
const mac = (input: KeyInput) => keyToCommand(input, true, 30);

describe("keyToCommand", () => {
  it("maps arrows to motions, extending with Shift", () => {
    expect(win(key("ArrowLeft"))).toEqual({
      kind: "op",
      op: { type: "move", motion: "left", extend: false },
    });
    expect(win(key("ArrowDown", { shiftKey: true }))).toEqual({
      kind: "op",
      op: { type: "move", motion: "down", extend: true },
    });
  });

  it("uses Ctrl for word movement on Windows and Linux, Option on macOS", () => {
    expect(win(key("ArrowRight", { ctrlKey: true }))).toMatchObject({
      op: { motion: "wordRight" },
    });
    expect(mac(key("ArrowRight", { altKey: true }))).toMatchObject({
      op: { motion: "wordRight" },
    });
    expect(mac(key("ArrowRight", { metaKey: true }))).toMatchObject({
      op: { motion: "lineEnd" },
    });
    expect(mac(key("ArrowUp", { metaKey: true }))).toMatchObject({
      op: { motion: "documentStart" },
    });
  });

  it("passes page size along with page motions", () => {
    expect(win(key("PageDown"))).toEqual({
      kind: "op",
      op: { type: "move", motion: "pageDown", extend: false, pageLines: 30 },
    });
  });

  it("maps deletion keys by unit", () => {
    expect(win(key("Backspace"))).toMatchObject({
      op: { type: "delete", direction: "backward", unit: "grapheme" },
    });
    expect(win(key("Delete", { ctrlKey: true }))).toMatchObject({
      op: { type: "delete", direction: "forward", unit: "word" },
    });
    expect(mac(key("Backspace", { altKey: true }))).toMatchObject({ op: { unit: "word" } });
  });

  it("adds cursors with Ctrl+Alt (Cmd+Option) and arrows", () => {
    expect(win(key("ArrowDown", { ctrlKey: true, altKey: true }))).toEqual({
      kind: "op",
      op: { type: "addCursor", direction: "down" },
    });
    expect(mac(key("ArrowUp", { metaKey: true, altKey: true }))).toMatchObject({
      op: { type: "addCursor", direction: "up" },
    });
  });

  it("maps editing shortcuts per platform", () => {
    expect(win(key("z", { ctrlKey: true }))).toMatchObject({ op: { type: "undo" } });
    expect(win(key("Z", { ctrlKey: true, shiftKey: true }))).toMatchObject({
      op: { type: "redo" },
    });
    expect(win(key("y", { ctrlKey: true }))).toMatchObject({ op: { type: "redo" } });
    expect(mac(key("z", { metaKey: true }))).toMatchObject({ op: { type: "undo" } });
    expect(mac(key("z", { ctrlKey: true }))).toBeNull();
    expect(win(key("c", { ctrlKey: true }))).toEqual({ kind: "clipboard", action: "copy" });
    expect(mac(key("x", { metaKey: true }))).toEqual({ kind: "clipboard", action: "cut" });
  });

  it("leaves text input, paste, and workbench shortcuts alone", () => {
    expect(win(key("a"))).toBeNull();
    expect(win(key("v", { ctrlKey: true }))).toBeNull();
    expect(win(key("s", { ctrlKey: true }))).toBeNull();
    expect(win(key("Enter", { ctrlKey: true }))).toBeNull();
  });

  it("keeps Shift+Tab in the editor without inserting anything", () => {
    expect(win(key("Tab", { shiftKey: true }))).toEqual({ kind: "ignore" });
    expect(win(key("Tab"))).toMatchObject({ op: { type: "tab" } });
  });
});
