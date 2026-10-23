import { describe, expect, it } from "vitest";
import type { LinesUpdate, WireView } from "../ipc/editor";
import { applyUpdate, knownLines, type LineCache } from "./lineCache";

function wire(version: number, startLine: number, endLine: number, lines: LinesUpdate): WireView {
  return {
    documentId: 1,
    version,
    lineCount: 1000,
    startLine,
    endLine,
    lines,
    selections: [],
    selectionCount: 1,
    primary: { position: { line: 0, column: 0 }, visualColumn: 0 },
    dirty: false,
    search: null,
  };
}

const texts = (cache: LineCache | null) => cache?.lines.map((line) => line.text);

const initial = applyUpdate(
  null,
  wire(1, 0, 5, {
    base: null,
    splices: [],
    chunks: [{ start: 0, lines: ["a", "b", "c", "d", "e"] }],
  }),
);

describe("applyUpdate", () => {
  it("fills an empty cache from a full update", () => {
    expect(texts(initial)).toEqual(["a", "b", "c", "d", "e"]);
    expect(knownLines(initial)).toEqual({ version: 1, startLine: 0, endLine: 5 });
  });

  it("replaces only an edited line", () => {
    const next = applyUpdate(
      initial,
      wire(2, 0, 5, {
        base: 1,
        splices: [{ start: 2, removed: 1, inserted: 1 }],
        chunks: [{ start: 2, lines: ["C"] }],
      }),
    );
    expect(texts(next)).toEqual(["a", "b", "C", "d", "e"]);
  });

  it("shifts held lines below an inserted line", () => {
    const next = applyUpdate(
      initial,
      wire(2, 0, 5, {
        base: 1,
        splices: [{ start: 1, removed: 1, inserted: 2 }],
        chunks: [{ start: 1, lines: ["b1", "b2"] }],
      }),
    );
    expect(texts(next)).toEqual(["a", "b1", "b2", "c", "d"]);
  });

  it("keeps overlapping lines when scrolling and adds new ones", () => {
    const next = applyUpdate(
      initial,
      wire(1, 3, 7, { base: 1, splices: [], chunks: [{ start: 5, lines: ["f", "g"] }] }),
    );
    expect(texts(next)).toEqual(["d", "e", "f", "g"]);
    expect(next?.startLine).toBe(3);
  });

  it("carries the full length of shortened lines", () => {
    const next = applyUpdate(
      null,
      wire(1, 0, 1, {
        base: null,
        splices: [],
        chunks: [{ start: 0, lines: [{ text: "abc", fullLength: 1_000_000 }] }],
      }),
    );
    expect(next?.lines[0]).toEqual({ text: "abc", fullLength: 1_000_000 });
  });

  it("refuses updates it cannot apply", () => {
    // The update is relative to a version the cache does not have.
    expect(applyUpdate(initial, wire(3, 0, 5, { base: 2, splices: [], chunks: [] }))).toBeNull();
    // A line the cache does not hold was not sent.
    expect(applyUpdate(initial, wire(1, 0, 6, { base: 1, splices: [], chunks: [] }))).toBeNull();
  });
});
