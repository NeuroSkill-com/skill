#!/usr/bin/env node
// Merge per-test JSON files emitted by `src/tests/visual-layout.spec.ts`
// into a single report.json + summary.txt.
//
// Why a separate script: Playwright's `afterAll` runs per-worker, so any
// in-memory aggregation captures only one worker's slice. Each test
// instead writes its own JSON, and this script merges them after the
// suite is done.

import * as fs from "node:fs";
import * as path from "node:path";

const OUT = path.join(process.cwd(), "test-results", "visual-layout");
const FINDINGS = path.join(OUT, "_findings");

if (!fs.existsSync(FINDINGS)) {
  console.error(`No findings directory at ${FINDINGS}. Run the visual spec first.`);
  process.exit(1);
}

const files = fs.readdirSync(FINDINGS).filter((f) => f.endsWith(".json"));
const results = files.map((f) => JSON.parse(fs.readFileSync(path.join(FINDINGS, f), "utf8")));

const issueCountsByKind = {};
for (const r of results) {
  for (const i of r.issues) {
    issueCountsByKind[i.kind] = (issueCountsByKind[i.kind] ?? 0) + 1;
  }
}

const sortedByIssues = [...results].sort((a, b) => b.issueCount - a.issueCount);

const summary = {
  total_combinations: results.length,
  issue_counts_by_kind: issueCountsByKind,
  combinations_with_issues: results.filter((r) => r.issueCount > 0).length,
  combinations_clean: results.filter((r) => r.issueCount === 0).length,
  worst_offenders: sortedByIssues.slice(0, 30).map((r) => ({
    route: r.route,
    viewport: r.viewport,
    scale: r.scale,
    issues: r.issueCount,
    screenshot: r.screenshot,
  })),
  results: sortedByIssues,
};

fs.writeFileSync(path.join(OUT, "report.json"), JSON.stringify(summary, null, 2));

const lines = [];
lines.push(`Visual layout audit — ${results.length} combinations`);
lines.push(`  clean: ${summary.combinations_clean}`);
lines.push(`  with issues: ${summary.combinations_with_issues}`);
lines.push("");
lines.push("Issue counts by kind:");
for (const [k, v] of Object.entries(issueCountsByKind)) {
  lines.push(`  ${k}: ${v}`);
}
lines.push("");
lines.push("Top 30 offenders:");
for (const o of summary.worst_offenders) {
  if (o.issues === 0) break;
  lines.push(`  ${o.issues.toString().padStart(3)}  ${o.route}  ${o.viewport}  ${o.scale}%`);
}

// Per-route worst-case totals — useful triage column.
const perRoute = {};
for (const r of results) {
  if (!perRoute[r.route]) perRoute[r.route] = { total: 0, max: 0 };
  perRoute[r.route].total += r.issueCount;
  perRoute[r.route].max = Math.max(perRoute[r.route].max, r.issueCount);
}
lines.push("");
lines.push("Per route (sum / max single combo):");
const routeOrder = Object.entries(perRoute).sort((a, b) => b[1].total - a[1].total);
for (const [route, stats] of routeOrder) {
  lines.push(`  ${route.padEnd(20)}  total=${stats.total.toString().padStart(4)}  worst=${stats.max}`);
}

fs.writeFileSync(path.join(OUT, "summary.txt"), lines.join("\n"));
console.log(lines.join("\n"));
