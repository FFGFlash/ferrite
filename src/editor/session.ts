/**
 * The frontend side of one open editor: sends operations to the backend in
 * order and delivers the resulting views.
 *
 * At most one request is in flight. Operations issued meanwhile are queued
 * and sent together as one batch, so fast typing costs few round trips and
 * never reorders. Every response carries the latest viewport's lines, so a
 * scroll during an edit needs no extra request.
 */
import type { ClipboardAction, EditorOp, EditorView, Viewport } from "../ipc/editor";
import * as ipc from "../ipc/editor";

export interface EditorTransport {
  apply(id: number, ops: EditorOp[], viewport: Viewport): Promise<EditorView>;
  view(id: number, viewport: Viewport): Promise<EditorView>;
  clipboard(id: number, action: ClipboardAction, viewport: Viewport): Promise<EditorView>;
}

export const ipcTransport: EditorTransport = {
  apply: ipc.applyOps,
  view: ipc.getView,
  clipboard: ipc.clipboard,
};

type Job = { kind: "ops"; ops: EditorOp[] } | { kind: "clipboard"; action: ClipboardAction };

export class EditorSession {
  private readonly queue: Job[] = [];
  private viewport: Viewport;
  private viewportStale = true;
  private busy = false;
  private disposed = false;
  private idleWaiters: (() => void)[] = [];

  constructor(
    private readonly documentId: number,
    viewport: Viewport,
    private readonly onView: (view: EditorView) => void,
    private readonly onError: (error: unknown) => void,
    private readonly transport: EditorTransport = ipcTransport,
  ) {
    this.viewport = viewport;
    this.pump();
  }

  dispatch(op: EditorOp): void {
    const last = this.queue.at(-1);
    if (last?.kind === "ops") {
      const previous = last.ops.at(-1);
      // While dragging, only the latest pointer position matters.
      if (isDragExtend(previous) && isDragExtend(op)) {
        last.ops[last.ops.length - 1] = op;
      } else {
        last.ops.push(op);
      }
    } else {
      this.queue.push({ kind: "ops", ops: [op] });
    }
    this.pump();
  }

  clipboard(action: ClipboardAction): void {
    this.queue.push({ kind: "clipboard", action });
    this.pump();
  }

  setViewport(viewport: Viewport): void {
    if (
      viewport.startLine === this.viewport.startLine &&
      viewport.endLine === this.viewport.endLine
    ) {
      return;
    }
    this.viewport = viewport;
    this.viewportStale = true;
    this.pump();
  }

  /** Resolves once every queued operation has been applied. */
  idle(): Promise<void> {
    if (!this.busy && this.queue.length === 0) {
      return Promise.resolve();
    }
    return new Promise((resolve) => this.idleWaiters.push(resolve));
  }

  dispose(): void {
    this.disposed = true;
    this.queue.length = 0;
    this.resolveIdle();
  }

  private pump(): void {
    if (this.busy || this.disposed) {
      return;
    }
    const job = this.queue.shift();
    if (!job && !this.viewportStale) {
      this.resolveIdle();
      return;
    }
    const viewport = this.viewport;
    this.viewportStale = false;
    this.busy = true;
    const request = !job
      ? this.transport.view(this.documentId, viewport)
      : job.kind === "ops"
        ? this.transport.apply(this.documentId, job.ops, viewport)
        : this.transport.clipboard(this.documentId, job.action, viewport);
    request
      .then((view) => {
        if (!this.disposed) {
          this.onView(view);
        }
      })
      .catch((error: unknown) => {
        if (!this.disposed) {
          // Resynchronize with the backend's state after a failure.
          this.viewportStale = true;
          this.onError(error);
        }
      })
      .finally(() => {
        this.busy = false;
        this.pump();
      });
  }

  private resolveIdle(): void {
    const waiters = this.idleWaiters;
    this.idleWaiters = [];
    for (const resolve of waiters) {
      resolve();
    }
  }
}

function isDragExtend(op: EditorOp | undefined): boolean {
  return op?.type === "setCursor" && op.extend && !op.add;
}
