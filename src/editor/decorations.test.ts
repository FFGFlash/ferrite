import { describe, expect, it } from "vitest";
import type { DecorationType } from "../ipc/extensions";
import { decorationRectStyle, textOverlays } from "./decorations";

const todo: DecorationType = {
  name: "todo",
  key: "example.todos/todo",
  dark: { background: "#e5c07b33", color: "#e5c07b", border: null, underlineColor: null },
  light: { background: "#98680122", color: "#986801", border: null, underlineColor: null },
  underline: "wavy",
  bold: true,
  italic: false,
};
const box: DecorationType = {
  ...todo,
  key: "example.todos/box",
  dark: { background: null, color: null, border: "url(x)", underlineColor: null },
  underline: null,
  bold: false,
};

describe("decorations", () => {
  it("styles rectangles per theme with validated colors only", () => {
    expect(decorationRectStyle(todo, "dark")).toEqual({
      "--decoration-background": "#e5c07b33",
      "--decoration-underline": "#e5c07b",
    });
    expect(decorationRectStyle(todo, "light")).toMatchObject({
      "--decoration-background": "#98680122",
    });
    expect(decorationRectStyle(box, "dark")).toEqual({});
  });

  it("splits text styles of multi-line decorations by line", () => {
    const types = new Map([
      [todo.key, todo],
      [box.key, box],
    ]);
    const overlays = textOverlays(
      [
        { start: { line: 1, column: 3 }, end: { line: 2, column: 2 }, key: todo.key },
        { start: { line: 1, column: 0 }, end: { line: 1, column: 1 }, key: box.key },
      ],
      types,
      "dark",
      1,
      [10, 5],
    );
    const style = { color: "#e5c07b", fontWeight: "bold" };
    expect(overlays.get(1)).toEqual([{ from: 3, to: 10, style }]);
    expect(overlays.get(2)).toEqual([{ from: 0, to: 2, style }]);
  });
});
