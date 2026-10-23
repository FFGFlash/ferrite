/**
 * End-to-end benchmarks that run inside the real app (IPC, rendering, and
 * painting included). Started with `--bench <name>` via `scripts/bench.mjs`;
 * results are reported to the backend, which writes them out and exits.
 */
import { activeEditor, type EditorController } from "../editor/activeEditor";
import { newDocument, openDocument, type DocumentSummary } from "../ipc/editor";
import {
  benchConfig,
  benchGenerate,
  benchReport,
  getPerfStats,
  type BenchConfig,
} from "../ipc/perf";
import { activate, documentsStore } from "../state/documents";
import { editorStatus } from "../state/editorStatus";
import { nextPaint, perfSnapshot, resetPerf, summarize } from "../state/perf";

type Benchmark = (config: BenchConfig) => Promise<Record<string, unknown>>;

const now = () => performance.now();
const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** Adds a document as the active tab and waits for its editor to paint. */
async function addDocument(summary: DocumentSummary): Promise<EditorController> {
  const painted = nextPaint((info) => info.documentId === summary.id);
  documentsStore.update((state) => ({
    ...state,
    documents: state.documents.some((d) => d.id === summary.id)
      ? state.documents
      : [...state.documents, summary],
  }));
  activate(summary.id);
  await painted;
  const editor = activeEditor.get();
  if (editor?.documentId !== summary.id) {
    throw new Error("the editor did not open");
  }
  return editor;
}

/** Types `count` characters one at a time, timing each until it is painted. */
async function typeAndTime(editor: EditorController, count: number): Promise<number[]> {
  const samples: number[] = [];
  let version = -1;
  for (let i = 0; i < count; i++) {
    const painted = nextPaint(
      (info) => info.documentId === editor.documentId && info.version > version,
    );
    const started = now();
    editor.dispatch({ type: "insert", text: i % 10 === 9 ? " " : "x" });
    version = (await painted).version;
    samples.push(now() - started);
  }
  return samples;
}

/**
 * Waits (up to a minute) until the active document's language server, if
 * any, has started and finished its initial work, so typing is measured in
 * the steady state rather than while the server indexes.
 */
async function languageServerSettled(): Promise<void> {
  const deadline = now() + 60_000;
  let readySince: number | null = null;
  while (now() < deadline) {
    const state = editorStatus.get()?.language?.server?.state;
    if (state === undefined || state === "failed") {
      return;
    }
    readySince = state === "ready" ? (readySince ?? now()) : null;
    if (readySince !== null && now() - readySince > 2_000) {
      return;
    }
    await delay(100);
  }
}

const benchmarks: Record<string, Benchmark> = {
  /** Time to an interactive window, measured once the app is idle. */
  async startup() {
    await delay(500);
    return { navigationToIdleMs: now(), frontend: perfSnapshot() };
  },

  /**
   * Keystroke-to-paint latency in a small and a large document, and a typing
   * burst. With a file, also typing in the middle of that file (which, for a
   * source file, includes syntax highlighting and language server sync).
   */
  async typing(config) {
    const small = await addDocument(await newDocument());
    await typeAndTime(small, 20);
    resetPerf();
    const smallSamples = await typeAndTime(small, 300);

    const burstPainted = nextPaint((info) => info.documentId === small.documentId);
    const burstStart = now();
    for (let i = 0; i < 300; i++) {
      small.dispatch({ type: "insert", text: "y" });
    }
    await burstPainted;
    // Wait until the last batch has been painted.
    let settled = now();
    for (;;) {
      const next = await Promise.race([nextPaint().then(() => true), delay(200).then(() => false)]);
      if (!next) {
        break;
      }
      settled = now();
    }
    const burstMs = settled - burstStart;

    const large = await addDocument(await benchGenerate(200_000, 80));
    large.dispatch({
      type: "setCursor",
      position: { line: 100_000, column: 10 },
      extend: false,
      add: false,
    });
    await nextPaint((info) => info.documentId === large.documentId && info.startLine > 1000);
    await typeAndTime(large, 20);
    const largeSamples = await typeAndTime(large, 300);

    let fileSamples: number[] | null = null;
    if (config.file) {
      const file = await addDocument(await openDocument(config.file));
      const middle = Math.floor(file.lineCount() / 2);
      file.dispatch({
        type: "setCursor",
        position: { line: middle, column: 0 },
        extend: false,
        add: false,
      });
      await languageServerSettled();
      await typeAndTime(file, 20);
      fileSamples = await typeAndTime(file, 300);
    }

    return {
      smallDocumentKeystroke: summarize("keystrokeToPaint", "ms", smallSamples),
      largeDocumentKeystroke: summarize("keystrokeToPaint", "ms", largeSamples),
      fileKeystroke: fileSamples && summarize("keystrokeToPaint", "ms", fileSamples),
      burst300CharactersMs: burstMs,
      frontend: perfSnapshot(),
      backend: await getPerfStats(),
    };
  },

  /** Page-jump scrolling through a 200,000-line document. */
  async scroll() {
    const generateStart = now();
    const summary = await benchGenerate(200_000, 80);
    const generateMs = now() - generateStart;
    const editor = await addDocument(summary);
    resetPerf();
    const samples: number[] = [];
    for (let page = 1; page <= 200; page++) {
      const target = page * 997;
      const painted = nextPaint(
        (info) =>
          info.documentId === editor.documentId &&
          info.startLine <= target &&
          target < info.endLine,
      );
      const started = now();
      editor.scrollToLine(target);
      await painted;
      samples.push(now() - started);
    }
    const endPainted = nextPaint(
      (info) => info.documentId === editor.documentId && info.endLine > 199_000,
    );
    const endStart = now();
    editor.dispatch({ type: "move", motion: "documentEnd", extend: false });
    await endPainted;
    return {
      generateAndOpen200kLinesMs: generateMs,
      pageJumpToPaint: summarize("scrollToPaint", "ms", samples),
      jumpToEndMs: now() - endStart,
      frontend: perfSnapshot(),
      backend: await getPerfStats(),
    };
  },

  /** Opening the file given on the command line, until its first paint. */
  async open(config) {
    if (!config.file) {
      throw new Error("the open benchmark needs a file: --bench open <file>");
    }
    const started = now();
    const summary = await openDocument(config.file);
    const loadedMs = now() - started;
    await addDocument(summary);
    return {
      openToLoadedMs: loadedMs,
      openToPaintMs: now() - started,
      backend: await getPerfStats(),
    };
  },
};

/** Runs the benchmark requested on the command line, if any. */
export async function runRequestedBenchmark(): Promise<void> {
  const config = await benchConfig();
  if (!config) {
    return;
  }
  const benchmark = benchmarks[config.name];
  try {
    if (!benchmark) {
      throw new Error(
        `unknown benchmark "${config.name}"; try ${Object.keys(benchmarks).join(", ")}`,
      );
    }
    await benchReport({ ok: true, ...(await benchmark(config)) });
  } catch (error) {
    await benchReport({ ok: false, error: String(error) });
  }
}
