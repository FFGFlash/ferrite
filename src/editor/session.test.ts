import { describe, expect, it, vi } from "vitest";
import type { EditorOp, EditorView, Viewport } from "../ipc/editor";
import { EditorSession, type EditorTransport } from "./session";

function view(version: number, viewport: Viewport): EditorView {
  return {
    documentId: 1,
    version,
    lineCount: 100,
    startLine: viewport.startLine,
    lines: [],
    selections: [],
    selectionCount: 1,
    primary: { position: { line: 0, column: 0 }, visualColumn: 0 },
    dirty: false,
  };
}

/** A transport whose requests resolve only when the test says so. */
function controlledTransport() {
  const calls: { kind: string; ops?: EditorOp[]; viewport: Viewport }[] = [];
  const resolvers: (() => void)[] = [];
  let version = 0;
  const respond = (kind: string, viewport: Viewport, ops?: EditorOp[]) => {
    calls.push({ kind, viewport, ...(ops ? { ops } : {}) });
    return new Promise<EditorView>((resolve) => {
      resolvers.push(() => resolve(view(++version, viewport)));
    });
  };
  const transport: EditorTransport = {
    apply: (_id, ops, viewport) => respond("apply", viewport, [...ops]),
    view: (_id, viewport) => respond("view", viewport),
    clipboard: (_id, _action, viewport) => respond("clipboard", viewport),
  };
  const next = async () => {
    resolvers.shift()?.();
    await new Promise((r) => setTimeout(r, 0));
  };
  return { transport, calls, next };
}

const insert = (text: string): EditorOp => ({ type: "insert", text });
const drag = (column: number): EditorOp => ({
  type: "setCursor",
  position: { line: 0, column },
  extend: true,
  add: false,
});

describe("EditorSession", () => {
  it("fetches the initial view and then batches operations sent while busy", async () => {
    const { transport, calls, next } = controlledTransport();
    const onView = vi.fn();
    const session = new EditorSession(1, { startLine: 0, endLine: 10 }, onView, vi.fn(), transport);

    session.dispatch(insert("a"));
    session.dispatch(insert("b"));
    session.dispatch(insert("c"));
    expect(calls).toHaveLength(1);
    expect(calls[0]?.kind).toBe("view");

    await next();
    expect(calls).toHaveLength(2);
    expect(calls[1]?.ops).toEqual([insert("a"), insert("b"), insert("c")]);

    await next();
    expect(onView).toHaveBeenCalledTimes(2);
    expect(calls).toHaveLength(2);
  });

  it("keeps only the latest drag position", async () => {
    const { transport, calls, next } = controlledTransport();
    const session = new EditorSession(
      1,
      { startLine: 0, endLine: 10 },
      vi.fn(),
      vi.fn(),
      transport,
    );
    session.dispatch(drag(1));
    session.dispatch(drag(2));
    session.dispatch(drag(3));
    await next();
    expect(calls[1]?.ops).toEqual([drag(3)]);
  });

  it("sends the latest viewport with the next request", async () => {
    const { transport, calls, next } = controlledTransport();
    const session = new EditorSession(
      1,
      { startLine: 0, endLine: 10 },
      vi.fn(),
      vi.fn(),
      transport,
    );
    session.setViewport({ startLine: 5, endLine: 15 });
    session.setViewport({ startLine: 20, endLine: 30 });
    session.dispatch(insert("x"));
    await next();
    expect(calls[1]).toMatchObject({ kind: "apply", viewport: { startLine: 20, endLine: 30 } });
    await next();
    expect(calls).toHaveLength(2);
  });

  it("reports errors, resynchronizes, and keeps processing", async () => {
    const onError = vi.fn();
    const onView = vi.fn();
    const transport: EditorTransport = {
      apply: vi.fn().mockRejectedValueOnce(new Error("stale")),
      view: vi.fn((_id, viewport: Viewport) => Promise.resolve(view(1, viewport))),
      clipboard: vi.fn(),
    };
    const session = new EditorSession(1, { startLine: 0, endLine: 10 }, onView, onError, transport);
    await session.idle();
    session.dispatch(insert("x"));
    await session.idle();
    expect(onError).toHaveBeenCalledOnce();
    expect(transport.view).toHaveBeenCalledTimes(2);
    expect(onView).toHaveBeenCalledTimes(2);
  });

  it("resolves idle once queued work is done", async () => {
    const { transport, next } = controlledTransport();
    const session = new EditorSession(
      1,
      { startLine: 0, endLine: 10 },
      vi.fn(),
      vi.fn(),
      transport,
    );
    session.dispatch(insert("a"));
    let idle = false;
    void session.idle().then(() => (idle = true));
    await next();
    expect(idle).toBe(false);
    await next();
    expect(idle).toBe(true);
  });
});
