import { afterEach, describe, expect, it } from "vitest";
import {
  nextPaint,
  notifyPainted,
  percentile,
  perfSnapshot,
  recordTiming,
  resetPerf,
  summarize,
} from "./perf";

describe("perf", () => {
  afterEach(resetPerf);

  it("summarizes samples with exact percentiles", () => {
    const values = Array.from({ length: 100 }, (_, i) => i + 1);
    const summary = summarize("x", "ms", values);
    expect(summary).toMatchObject({ count: 100, mean: 50.5, p50: 50, p95: 95, p99: 99, max: 100 });
    expect(percentile([], 0.5)).toBe(0);
  });

  it("records named series", () => {
    recordTiming("ipc.roundTrip", 2);
    recordTiming("ipc.roundTrip", 4);
    const [series] = perfSnapshot();
    expect(series).toMatchObject({ name: "ipc.roundTrip", count: 2, mean: 3, max: 4 });
  });

  it("resolves paint waiters that match", async () => {
    const waiting = nextPaint((info) => info.version === 2);
    notifyPainted({ documentId: 1, version: 1, startLine: 0, endLine: 10 });
    notifyPainted({ documentId: 1, version: 2, startLine: 0, endLine: 10 });
    await expect(waiting).resolves.toMatchObject({ version: 2 });
  });
});
