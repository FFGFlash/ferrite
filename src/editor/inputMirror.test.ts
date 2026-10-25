import { describe, expect, it } from "vitest";
import type { EditorView, Position, SelectionView } from "../ipc/editor";
import { MIRROR_MAX_LINES, inputMirror, typedText } from "./inputMirror";

function view(texts: string[], selection: Omit<SelectionView, "primary">, startLine = 0) {
  return {
    startLine,
    endLine: startLine + texts.length,
    lines: texts.map((text) => ({ text, fullLength: null, spans: [] })),
    selections: [{ ...selection, primary: true }],
  } as unknown as EditorView;
}

const at = (line: number, column: number): Position => ({ line, column });

describe("inputMirror", () => {
  it("holds the cursor's line with the cursor in it", () => {
    const mirror = inputMirror(
      view(["fn main() {", "  ok();", "}"], { anchor: at(1, 4), head: at(1, 4) }),
    );
    expect(mirror).toEqual({
      line: 1,
      value: "  ok();",
      selectionStart: 4,
      selectionEnd: 4,
      direction: "forward",
    });
  });

  it("holds the lines of a selection, in its direction", () => {
    const mirror = inputMirror(
      view(["zero", "one", "two", "three"], { anchor: at(3, 2), head: at(1, 1) }),
    );
    expect(mirror).toEqual({
      line: 1,
      value: "one\ntwo\nthree",
      selectionStart: 1,
      selectionEnd: 10,
      direction: "backward",
    });
  });

  it("limits large selections to the lines near the cursor", () => {
    const texts = Array.from({ length: 1000 }, (_, i) => `line ${i}`);
    const mirror = inputMirror(view(texts, { anchor: at(0, 0), head: at(900, 3) }))!;
    expect(mirror.line).toBe(900 - MIRROR_MAX_LINES / 2);
    expect(mirror.selectionStart).toBe(0);
    expect(mirror.value.endsWith("line 900")).toBe(true);
    expect(mirror.selectionEnd).toBe(mirror.value.length - "line 900".length + 3);

    // Only lines the view holds are mirrored.
    const partial = inputMirror(view(["a", "b"], { anchor: at(0, 0), head: at(11, 1) }, 10))!;
    expect(partial).toMatchObject({ line: 10, value: "a\nb", selectionStart: 0, selectionEnd: 3 });
  });

  it("mirrors nothing when the cursor is out of view or on a shortened line", () => {
    expect(inputMirror(view(["a"], { anchor: at(5, 0), head: at(5, 0) }))).toBeNull();
    const long = view(["x"], { anchor: at(0, 0), head: at(0, 0) });
    long.lines[0] = { text: "x", fullLength: 20_000, spans: [] };
    expect(inputMirror(long)).toBeNull();
  });
});

describe("typedText", () => {
  const mirror = {
    line: 0,
    value: "let x = 1;",
    selectionStart: 4,
    selectionEnd: 5,
    direction: "forward" as const,
  };

  it("finds what replaced the selection", () => {
    expect(typedText(mirror, "let yz = 1;")).toBe("yz");
    expect(typedText(mirror, "let  = 1;")).toBe("");
  });

  it("refuses changes elsewhere", () => {
    expect(typedText(mirror, "let x = 2;")).toBeNull();
    expect(typedText(mirror, "")).toBeNull();
  });
});
