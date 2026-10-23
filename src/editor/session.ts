/**
 * The frontend side of one open editor: sends operations to the backend in
 * order and delivers the resulting views.
 *
 * At most one request is in flight. Operations issued meanwhile are queued
 * and sent together as one batch, so fast typing costs few round trips and
 * never reorders. Each request says which lines the session already holds,
 * so the backend sends only the lines it is missing (see `lineCache.ts`).
 */
import type {
  ClipboardAction,
  EditorOp,
  EditorView,
  KnownLines,
  SearchQuery,
  Viewport,
  WireView,
} from "../ipc/editor";
import * as ipc from "../ipc/editor";
import { log } from "../ipc";
import { recordCount, recordTiming } from "../state/perf";
import { applyUpdate, knownLines, type LineCache } from "./lineCache";

export interface EditorTransport {
  apply(
    id: number,
    ops: EditorOp[],
    viewport: Viewport,
    known: KnownLines | null,
    search: SearchQuery | null,
  ): Promise<WireView>;
  view(
    id: number,
    viewport: Viewport,
    known: KnownLines | null,
    search: SearchQuery | null,
  ): Promise<WireView>;
  clipboard(
    id: number,
    action: ClipboardAction,
    viewport: Viewport,
    known: KnownLines | null,
    search: SearchQuery | null,
  ): Promise<WireView>;
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
  private search: SearchQuery | null = null;
  private cache: LineCache | null = null;
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
    // Start on a microtask so operations dispatched right after construction
    // (e.g. a selection to restore) go out with the first request.
    queueMicrotask(() => this.pump());
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

  /**
   * Sets the find query whose matches every view should include, or `null`
   * for none. Fetches a fresh view.
   */
  setSearch(search: SearchQuery | null): void {
    const next = search && search.pattern ? search : null;
    if (JSON.stringify(next) === JSON.stringify(this.search)) {
      return;
    }
    this.search = next;
    this.refresh();
  }

  /** Fetches a fresh view, e.g. after the document changed elsewhere. */
  refresh(): void {
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
    const { viewport, search } = this;
    const known = knownLines(this.cache);
    this.viewportStale = false;
    this.busy = true;
    const sent = performance.now();
    const request = !job
      ? this.transport.view(this.documentId, viewport, known, search)
      : job.kind === "ops"
        ? this.transport.apply(this.documentId, job.ops, viewport, known, search)
        : this.transport.clipboard(this.documentId, job.action, viewport, known, search);
    request
      .then((wire) => {
        recordTiming("ipc.roundTrip", performance.now() - sent);
        if (!this.disposed) {
          this.receive(wire);
        }
      })
      .catch((error: unknown) => {
        if (!this.disposed) {
          // Resynchronize with the backend's state after a failure.
          this.cache = null;
          this.viewportStale = true;
          this.onError(error);
        }
      })
      .finally(() => {
        this.busy = false;
        this.pump();
      });
  }

  private receive(wire: WireView): void {
    recordCount(
      "lines.received",
      wire.lines.chunks.reduce((sum, chunk) => sum + chunk.lines.length, 0),
    );
    const cache = applyUpdate(this.cache, wire);
    if (!cache) {
      // Should not happen; recover by asking for every line again.
      log("warn", `Editor ${this.documentId}: line update did not apply; refetching`);
      this.cache = null;
      this.viewportStale = true;
      return;
    }
    this.cache = cache;
    this.onView({ ...wire, lines: cache.lines });
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
