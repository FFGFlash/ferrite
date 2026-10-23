/** Typed wrappers for performance statistics and benchmark mode (`src-tauri/src/{perf,bench}.rs`). */
import type { DocumentSummary } from "./editor";
import { call, isDesktop } from "./index";

export interface Timing {
  name: string;
  count: number;
  meanUs: number;
  p50Us: number;
  p95Us: number;
  p99Us: number;
  maxUs: number;
}

export interface TaskStats {
  name: string;
  completed: number;
  cancelled: number;
  dropped: number;
  panicked: number;
  waitP95Us: number;
  runP50Us: number;
  runP95Us: number;
  runMaxUs: number;
}

export interface PerfStats {
  timings: Timing[];
  tasks: TaskStats[];
  queuedTasks: number;
  /** Backend process only; WebView processes are separate. */
  backendMemoryBytes: number | null;
  openDocuments: number;
  openDocumentBytes: number;
}

export interface BenchConfig {
  name: string;
  file: string | null;
}

export const getPerfStats = () => call<PerfStats>("perf_stats");

export const benchConfig = (): Promise<BenchConfig | null> =>
  isDesktop() ? call<BenchConfig | null>("bench_config") : Promise.resolve(null);
export const benchGenerate = (lines: number, width: number) =>
  call<DocumentSummary>("bench_generate", { lines, width });
export const benchReport = (result: unknown) => call<void>("bench_report", { result });
