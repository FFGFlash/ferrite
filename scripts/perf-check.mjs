#!/usr/bin/env node
/**
 * Performance regression checks.
 *
 *   node scripts/perf-check.mjs                  # compare the last end-to-end run
 *                                                (target/bench/results.json, from
 *                                                scripts/bench.mjs) with the
 *                                                committed baseline
 *   node scripts/perf-check.mjs --update         # make the last run the baseline
 *   node scripts/perf-check.mjs micro            # check Criterion's comparisons
 *                                                after `cargo bench -p ferrite-benches
 *                                                -- --baseline <name>`
 *
 * Options: --results PATH, --baseline-file PATH (default benches/baseline.json),
 * --binary PATH, and for micro, --threshold PERCENT (default 10).
 *
 * End-to-end timings are noisy, so each metric has a tolerance: a relative
 * margin plus an absolute slack (a frame for paint latency). Results from a
 * different machine than the baseline's are compared but flagged. Exits with
 * status 1 when a metric regressed beyond its tolerance.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const index = args.indexOf(`--${name}`);
  return index >= 0 && args[index + 1] ? args[index + 1] : fallback;
};
const mode = args[0] === "micro" ? "micro" : "app";

const MB = 1024 * 1024;
/** Allowed growth per kind of metric: `ratio` of the baseline plus `slack`. */
const TOLERANCE = {
  time: { ratio: 0.25, slack: 5 },
  // Paint latency is quantized to display frames; allow jitter, not a frame.
  frame: { ratio: 0.1, slack: 2 },
  backend: { ratio: 0.5, slack: 0.1 },
  memory: { ratio: 0.15, slack: 8 * MB },
  size: { ratio: 0.1, slack: 0 },
};

const timing = (suite, name, field) => (results) =>
  results[suite]?.backend?.timings?.find((entry) => entry.name === name)?.[field] / 1000;

const METRICS = [
  [
    "Startup, later runs (median)",
    "time",
    "ms",
    (r) => r.startup?.laterRuns?.launchToReadyMedianMs,
  ],
  [
    "Idle memory, process tree",
    "memory",
    "bytes",
    (r) => r.startup?.laterRuns?.treeResidentMedianBytes,
  ],
  ["Idle memory, backend", "memory", "bytes", (r) => r.startup?.laterRuns?.appResidentMedianBytes],
  ["Keystroke → paint, small (p50)", "frame", "ms", (r) => r.typing?.smallDocumentKeystroke?.p50],
  ["Keystroke → paint, small (p95)", "frame", "ms", (r) => r.typing?.smallDocumentKeystroke?.p95],
  [
    "Keystroke → paint, 200k lines (p50)",
    "frame",
    "ms",
    (r) => r.typing?.largeDocumentKeystroke?.p50,
  ],
  [
    "Keystroke → paint, 200k lines (p95)",
    "frame",
    "ms",
    (r) => r.typing?.largeDocumentKeystroke?.p95,
  ],
  ["300-character burst", "time", "ms", (r) => r.typing?.burst300CharactersMs],
  ["Backend editor.apply (p95)", "backend", "ms", timing("typing", "editor.apply", "p95Us")],
  ["Page jump → paint (p50)", "frame", "ms", (r) => r.scroll?.pageJumpToPaint?.p50],
  ["Page jump → paint (p95)", "frame", "ms", (r) => r.scroll?.pageJumpToPaint?.p95],
  ["Generate + open 200k lines", "time", "ms", (r) => r.scroll?.generateAndOpen200kLinesMs],
  ["Binary size", "size", "bytes", (r) => r.binaryBytes],
];

const formatValue = (value, unit) =>
  value == null
    ? "—"
    : unit === "bytes"
      ? `${(value / MB).toFixed(1)} MB`
      : `${value.toFixed(1)} ms`;

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function appCheck() {
  const resultsFile = path.resolve(
    option("results", path.join(root, "target", "bench", "results.json")),
  );
  const baselineFile = path.resolve(
    option("baseline-file", path.join(root, "benches", "baseline.json")),
  );
  const binary = path.resolve(
    option(
      "binary",
      path.join(
        root,
        "target",
        "release",
        process.platform === "win32" ? "ferrite.exe" : "ferrite",
      ),
    ),
  );
  if (!fs.existsSync(resultsFile)) {
    console.error(`No results at ${resultsFile}. Run: node scripts/bench.mjs all`);
    process.exit(2);
  }
  const results = readJson(resultsFile);
  if (fs.existsSync(binary)) {
    results.binaryBytes = fs.statSync(binary).size;
  }
  const current = Object.fromEntries(
    METRICS.map(([label, , , get]) => [label, get(results)]).filter(([, value]) =>
      Number.isFinite(value),
    ),
  );

  if (args.includes("--update")) {
    const baseline = { machine: results.machine, date: results.date, metrics: current };
    fs.writeFileSync(baselineFile, `${JSON.stringify(baseline, null, 2)}\n`);
    console.log(
      `Recorded ${Object.keys(current).length} metrics in ${path.relative(root, baselineFile)}`,
    );
    return 0;
  }
  if (!fs.existsSync(baselineFile)) {
    console.error(`No baseline at ${baselineFile}. Record one with --update.`);
    process.exit(2);
  }
  const baseline = readJson(baselineFile);
  if (
    baseline.machine?.cpu !== results.machine?.cpu ||
    baseline.machine?.platform !== results.machine?.platform
  ) {
    console.warn(
      `Note: the baseline was measured on ${baseline.machine?.cpu?.trim()} (${baseline.machine?.platform}), ` +
        `this run on ${results.machine?.cpu?.trim()} (${results.machine?.platform}); differences may be the machine's.`,
    );
  }

  let regressions = 0;
  console.log(
    `\n${"Metric".padEnd(38)}${"Baseline".padStart(12)}${"Now".padStart(12)}${"Change".padStart(10)}`,
  );
  for (const [label, kind, unit] of METRICS) {
    const before = baseline.metrics?.[label];
    const now = current[label];
    if (before == null || now == null) {
      console.log(
        `${label.padEnd(38)}${formatValue(before, unit).padStart(12)}${formatValue(now, unit).padStart(12)}   not measured`,
      );
      continue;
    }
    const { ratio, slack } = TOLERANCE[kind];
    const limit = before * (1 + ratio) + slack;
    const change = before === 0 ? 0 : ((now - before) / before) * 100;
    const regressed = now > limit;
    regressions += regressed ? 1 : 0;
    console.log(
      `${label.padEnd(38)}${formatValue(before, unit).padStart(12)}${formatValue(now, unit).padStart(12)}` +
        `${`${change >= 0 ? "+" : ""}${change.toFixed(0)}%`.padStart(10)}` +
        (regressed ? `   REGRESSED (limit ${formatValue(limit, unit)})` : ""),
    );
  }
  console.log(
    regressions
      ? `\n${regressions} metric${regressions === 1 ? "" : "s"} regressed beyond tolerance.`
      : "\nNo regressions beyond tolerance.",
  );
  return regressions ? 1 : 0;
}

/**
 * Benchmarks Criterion compared in its latest run, found as benchmark
 * directories (`target/criterion/<group>/<bench>/...`) with both a fresh
 * `new/estimates.json` and the comparison it wrote next to it.
 */
function criterionChanges(dir, name = "") {
  const found = [];
  const change = path.join(dir, "change", "estimates.json");
  const fresh = path.join(dir, "new", "estimates.json");
  if (
    fs.existsSync(change) &&
    fs.existsSync(fresh) &&
    fs.statSync(change).mtimeMs >= fs.statSync(fresh).mtimeMs - 1000
  ) {
    found.push({ name, mean: readJson(change).mean });
  }
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory() && !["report", "new", "base", "change"].includes(entry.name)) {
      found.push(
        ...criterionChanges(
          path.join(dir, entry.name),
          name ? `${name}/${entry.name}` : entry.name,
        ),
      );
    }
  }
  return found;
}

function microCheck() {
  const threshold = Number(option("threshold", "10")) / 100;
  const dir = path.join(root, "target", "criterion");
  if (!fs.existsSync(dir)) {
    console.error("No Criterion results. Run: cargo bench -p ferrite-benches -- --baseline <name>");
    process.exit(2);
  }
  const changes = criterionChanges(dir).sort((a, b) => a.name.localeCompare(b.name));
  if (changes.length === 0) {
    console.error(
      "Criterion recorded no comparisons; run the benchmarks against a saved baseline first.",
    );
    process.exit(2);
  }
  let regressions = 0;
  for (const { name, mean } of changes) {
    // Regressed only if even the optimistic end of the interval is slower.
    const lower = mean.confidence_interval.lower_bound;
    const regressed = lower > threshold;
    regressions += regressed ? 1 : 0;
    const pct = (value) => `${value >= 0 ? "+" : ""}${(value * 100).toFixed(1)}%`;
    console.log(
      `${name.padEnd(60)}${pct(mean.point_estimate).padStart(9)}  [${pct(lower)}, ${pct(mean.confidence_interval.upper_bound)}]` +
        (regressed ? "  REGRESSED" : ""),
    );
  }
  console.log(
    regressions
      ? `\n${regressions} benchmark${regressions === 1 ? "" : "s"} slower by more than ${threshold * 100}%.`
      : `\nNo benchmark slower by more than ${threshold * 100}% (${changes.length} compared, ${os.cpus()[0]?.model?.trim()}).`,
  );
  return regressions ? 1 : 0;
}

process.exit(mode === "micro" ? microCheck() : appCheck());
