#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// npm run health — deep inspection of ~/.skill for debugging session listing,
// history views, search indices, and data integrity.

import { readdirSync, statSync, existsSync, readFileSync } from "fs";
import { join, extname } from "path";
import { homedir } from "os";

const SKILL_DIR = process.env.SKILL_DIR || join(homedir(), ".skill");
const B = "\x1b[1m", D = "\x1b[2m", R = "\x1b[0m";
const G = "\x1b[32m", Y = "\x1b[33m", RED = "\x1b[31m", C = "\x1b[36m", M = "\x1b[35m";

// ── Helpers ───────────────────────────────────────────────────────────────────

function fmtB(b) {
  if (b >= 1e9) return `${(b / 1e9).toFixed(2)} GB`;
  if (b >= 1e6) return `${(b / 1e6).toFixed(1)} MB`;
  if (b >= 1e3) return `${(b / 1e3).toFixed(1)} KB`;
  return `${b} B`;
}
function fmtDur(s) {
  if (s >= 3600) return `${Math.floor(s/3600)}h ${Math.floor((s%3600)/60)}m ${s%60}s`;
  if (s >= 60) return `${Math.floor(s/60)}m ${s%60}s`;
  return `${s}s`;
}
function fmtTs(utc) {
  return new Date(utc * 1000).toLocaleString(undefined, {
    month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false,
  });
}
function fmtLocalDay(utc) {
  const d = new Date(utc * 1000);
  return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,"0")}-${String(d.getDate()).padStart(2,"0")}`;
}
function dirSize(dir) {
  let t = 0;
  try { for (const f of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, f.name);
    if (f.isDirectory()) t += dirSize(p); else if (f.isFile()) t += statSync(p).size;
  }} catch {} return t;
}
function isDateDir(n) { return /^\d{8}$/.test(n); }
function section(t) { console.log(`\n${B}── ${t} ${"─".repeat(Math.max(0, 62 - t.length))}${R}`); }
function row(l, v, c = "") { console.log(`  ${D}${l.padEnd(36)}${R} ${c}${v}${c ? R : ""}`); }
function ok(m) { row("✓", m, G); }
function warn(m) { row("⚠", m, Y); }

// ── Main ──────────────────────────────────────────────────────────────────────

if (!existsSync(SKILL_DIR)) { console.log(`${RED}~/.skill not found at ${SKILL_DIR}${R}`); process.exit(1); }

const tzOff = new Date().getTimezoneOffset();
const tzName = Intl.DateTimeFormat().resolvedOptions().timeZone;

console.log(`${B}NeuroSkill™ Health Report${R}`);
console.log(`${D}${SKILL_DIR}${R}`);
console.log(`${D}Timezone: ${tzName} (UTC${tzOff <= 0 ? "+" : ""}${-tzOff/60}h)  Local: ${new Date().toLocaleString()}${R}`);

// ── Scan all day directories ─────────────────────────────────────────────────

const allEntries = readdirSync(SKILL_DIR, { withFileTypes: true });
const dayDirs = allEntries.filter(e => e.isDirectory() && isDateDir(e.name)).map(e => e.name).sort();

/** @type {Array<{day:string, sessions:Array<any>, files:any, size:number}>} */
const dayData = [];

let totSessions = 0, totCsv = 0, totParquet = 0, totMetrics = 0, totPpg = 0, totImu = 0;
let totLogs = 0, totSqlite = 0, totHnsw = 0, totBytes = 0, totMetricsCache = 0;
let totMeta = 0; // .meta.jsonl

for (const day of dayDirs) {
  const dp = join(SKILL_DIR, day);
  const files = readdirSync(dp);
  const sessions = [];
  const fileCounts = { csv: 0, parquet: 0, metrics: 0, ppg: 0, imu: 0, log: 0, sqlite: 0, hnsw: 0, metricsCache: 0, metaJsonl: 0 };
  let dayBytes = 0;

  for (const f of files) {
    const fp = join(dp, f);
    try { dayBytes += statSync(fp).size; } catch {}

    // Session JSON sidecar
    if ((f.startsWith("exg_") || f.startsWith("muse_")) && f.endsWith(".json") && !f.endsWith("_metrics_cache.json")) {
      try {
        const j = JSON.parse(readFileSync(join(dp, f), "utf-8"));
        const csvFile = j.csv_file || "";
        const csvExists = csvFile && existsSync(join(dp, csvFile));
        const pqFile = csvFile.replace(/\.csv$/, ".parquet");
        const pqExists = pqFile && existsSync(join(dp, pqFile));
        const metricsFile = csvFile.replace(/\.csv$/, "_metrics.csv");
        const metricsExists = existsSync(join(dp, metricsFile)) || existsSync(join(dp, csvFile.replace(/\.csv$/, "_metrics.parquet")));
        const ppgFile = csvFile.replace(/\.csv$/, "_ppg.csv");
        const ppgExists = existsSync(join(dp, ppgFile)) || existsSync(join(dp, csvFile.replace(/\.csv$/, "_ppg.parquet")));
        const imuFile = csvFile.replace(/\.csv$/, "_imu.csv");
        const imuExists = existsSync(join(dp, imuFile)) || existsSync(join(dp, csvFile.replace(/\.csv$/, "_imu.parquet")));
        const metaJsonl = existsSync(join(dp, csvFile.replace(/\.csv$/, ".meta.jsonl")));

        let csvSize = 0;
        if (pqExists) try { csvSize = statSync(join(dp, pqFile)).size; } catch {}
        else if (csvExists) try { csvSize = statSync(join(dp, csvFile)).size; } catch {}

        sessions.push({
          file: f,
          csv_file: csvFile,
          start: j.session_start_utc ?? null,
          end: j.session_end_utc ?? null,
          dur: (j.session_start_utc && j.session_end_utc) ? j.session_end_utc - j.session_start_utc : null,
          device: j.device?.name || j.device_name || null,
          samples: j.total_samples ?? null,
          sample_rate: j.sample_rate_hz ?? null,
          snr: j.avg_snr_db ?? null,
          csvExists, pqExists, metricsExists, ppgExists, imuExists, metaJsonl, csvSize,
        });
      } catch { sessions.push({ file: f, error: "parse error" }); }
    }

    // Count files
    const ext = extname(f);
    if (ext === ".csv" && !f.includes("_metrics") && !f.includes("_ppg") && !f.includes("_imu")) fileCounts.csv++;
    if (ext === ".parquet" && !f.includes("_metrics") && !f.includes("_ppg") && !f.includes("_imu")) fileCounts.parquet++;
    if (f.includes("_metrics.csv") || f.includes("_metrics.parquet")) fileCounts.metrics++;
    if (f.includes("_ppg.csv") || f.includes("_ppg.parquet")) fileCounts.ppg++;
    if (f.includes("_imu.csv") || f.includes("_imu.parquet")) fileCounts.imu++;
    if (f.startsWith("log_") && ext === ".txt") fileCounts.log++;
    if (f.endsWith(".sqlite") || f.endsWith(".sqlite-wal") || f.endsWith(".sqlite-shm")) fileCounts.sqlite++;
    if (f.endsWith(".hnsw")) fileCounts.hnsw++;
    if (f.endsWith("_metrics_cache.json")) fileCounts.metricsCache++;
    if (f.endsWith(".meta.jsonl")) fileCounts.metaJsonl++;
  }

  totSessions += sessions.length; totCsv += fileCounts.csv; totParquet += fileCounts.parquet;
  totMetrics += fileCounts.metrics; totPpg += fileCounts.ppg; totImu += fileCounts.imu;
  totLogs += fileCounts.log; totSqlite += fileCounts.sqlite; totHnsw += fileCounts.hnsw;
  totBytes += dayBytes; totMetricsCache += fileCounts.metricsCache; totMeta += fileCounts.metaJsonl;

  dayData.push({ day, sessions, files: fileCounts, size: dayBytes });
}

// ── Summary ──────────────────────────────────────────────────────────────────

section("Overview");
row("Day directories", String(dayDirs.length), C);
row("Days with sessions", `${dayData.filter(d => d.sessions.length > 0).length} / ${dayDirs.length}`, G);
row("Total sessions", String(totSessions), C);
row("Date range (UTC dirs)", dayDirs.length > 0 ? `${dayDirs[0]} → ${dayDirs[dayDirs.length - 1]}` : "none");
row("Recording data size", fmtB(totBytes));

// ── File breakdown ───────────────────────────────────────────────────────────

section("File Counts");
row("EEG data (CSV)", String(totCsv));
row("EEG data (Parquet)", String(totParquet));
row("Metrics files", String(totMetrics));
row("Metrics cache (JSON)", String(totMetricsCache));
row("PPG files", String(totPpg));
row("IMU files", String(totImu));
row("Meta JSONL files", String(totMeta));
row("Log files", String(totLogs));
row("SQLite DBs (per-day)", String(totSqlite));
row("HNSW indices (per-day)", String(totHnsw));

// ── Per-day detail ───────────────────────────────────────────────────────────

section("Per-Day Breakdown");

// Collect all sessions for local-day grouping
const allSessions = [];
for (const dd of dayData) {
  for (const s of dd.sessions) {
    allSessions.push({ ...s, utcDir: dd.day });
  }
}
// Group by local day
const byLocalDay = new Map();
for (const s of allSessions) {
  if (!s.start) continue;
  const ld = fmtLocalDay(s.start);
  if (!byLocalDay.has(ld)) byLocalDay.set(ld, []);
  byLocalDay.get(ld).push(s);
}
const localDays = [...byLocalDay.keys()].sort().reverse();

console.log(`\n  ${D}Local days (from session timestamps): ${localDays.length}${R}`);
console.log(`  ${D}${"Local Day".padEnd(12)} ${"Sessions".padEnd(10)} ${"Duration".padEnd(14)} ${"Time Range".padEnd(30)} Files${R}`);
console.log(`  ${D}${"─".repeat(90)}${R}`);

for (const ld of localDays) {
  const sess = byLocalDay.get(ld);
  sess.sort((a, b) => (a.start || 0) - (b.start || 0));
  const totalDur = sess.reduce((a, s) => a + (s.dur || 0), 0);
  const first = sess[0], last = sess[sess.length - 1];
  const timeRange = first.start && last.end ? `${fmtTs(first.start)} → ${fmtTs(last.end)}` : "?";
  const fileFlags = [];
  const hasAllCsv = sess.every(s => s.csvExists || s.pqExists);
  const hasAllMetrics = sess.every(s => s.metricsExists);
  const hasSomePpg = sess.some(s => s.ppgExists);
  const hasSomeImu = sess.some(s => s.imuExists);
  if (!hasAllCsv) fileFlags.push(`${RED}missing-data${R}`);
  if (!hasAllMetrics) fileFlags.push(`${Y}missing-metrics${R}`);
  if (hasSomePpg) fileFlags.push("ppg");
  if (hasSomeImu) fileFlags.push("imu");

  console.log(`  ${C}${ld}${R}     ${String(sess.length).padEnd(10)} ${fmtDur(totalDur).padEnd(14)} ${timeRange.padEnd(30)} ${fileFlags.join(" ")}`);
}

// ── Per-UTC-dir detail ───────────────────────────────────────────────────────

section("Per UTC-Dir Detail");
console.log(`  ${D}${"Dir".padEnd(10)} ${"Sess".padEnd(5)} ${"CSV".padEnd(4)} ${"PQ".padEnd(4)} ${"Met".padEnd(4)} ${"PPG".padEnd(4)} ${"IMU".padEnd(4)} ${"Log".padEnd(4)} ${"DB".padEnd(4)} ${"HNSW".padEnd(5)} Size${R}`);
console.log(`  ${D}${"─".repeat(75)}${R}`);

for (const dd of dayData) {
  const f = dd.files;
  const sessN = dd.sessions.length;
  const color = sessN > 0 ? "" : D;
  console.log(`  ${color}${dd.day.padEnd(10)} ${String(sessN).padEnd(5)} ${String(f.csv).padEnd(4)} ${String(f.parquet).padEnd(4)} ${String(f.metrics).padEnd(4)} ${String(f.ppg).padEnd(4)} ${String(f.imu).padEnd(4)} ${String(f.log).padEnd(4)} ${String(f.sqlite).padEnd(4)} ${String(f.hnsw).padEnd(5)} ${fmtB(dd.size)}${color ? R : ""}`);
}

// ── Session detail table ─────────────────────────────────────────────────────

section("Session Detail (all sessions, newest first)");
allSessions.sort((a, b) => (b.start || 0) - (a.start || 0));

console.log(`  ${D}${"#".padEnd(4)} ${"UTC Dir".padEnd(10)} ${"Local Day".padEnd(12)} ${"Start".padEnd(22)} ${"Duration".padEnd(10)} ${"Device".padEnd(14)} ${"SNR".padEnd(6)} ${"Data".padEnd(6)} ${"Met".padEnd(4)} ${"PPG".padEnd(4)} ${"IMU".padEnd(4)} Size${R}`);
console.log(`  ${D}${"─".repeat(110)}${R}`);

for (let i = 0; i < allSessions.length; i++) {
  const s = allSessions[i];
  if (s.error) { console.log(`  ${RED}${String(i+1).padEnd(4)} ${s.utcDir.padEnd(10)} ${s.file} — ${s.error}${R}`); continue; }

  const ld = s.start ? fmtLocalDay(s.start) : "?";
  const startStr = s.start ? fmtTs(s.start) : "no timestamp";
  const durStr = s.dur ? fmtDur(s.dur) : "?";
  const dev = (s.device || "").slice(0, 12).padEnd(14);
  const snr = s.snr != null ? `${s.snr.toFixed(1)}dB` : "—";
  const dataOk = (s.csvExists || s.pqExists) ? `${G}✓${R}` : `${RED}✗${R}`;
  const metOk = s.metricsExists ? `${G}✓${R}` : `${Y}✗${R}`;
  const ppgOk = s.ppgExists ? `${G}✓${R}` : `${D}—${R}`;
  const imuOk = s.imuExists ? `${G}✓${R}` : `${D}—${R}`;
  const sz = s.csvSize > 0 ? fmtB(s.csvSize) : `${D}0${R}`;

  // Highlight sessions with missing timestamps (these would be invisible in history)
  const tsColor = s.start ? "" : RED;
  console.log(`  ${tsColor}${String(i+1).padEnd(4)} ${s.utcDir.padEnd(10)} ${ld.padEnd(12)} ${startStr.padEnd(22)} ${durStr.padEnd(10)} ${dev} ${snr.padEnd(6)} ${dataOk.padEnd(6)}  ${metOk.padEnd(4)}  ${ppgOk.padEnd(4)}  ${imuOk.padEnd(4)}  ${sz}${tsColor ? R : ""}`);
}

// ── Orphan analysis ──────────────────────────────────────────────────────────

section("Data Integrity");

const noTimestamp = allSessions.filter(s => !s.start);
const noData = allSessions.filter(s => !s.error && !s.csvExists && !s.pqExists);
const noMetrics = allSessions.filter(s => !s.error && !s.metricsExists);
const emptyDays = dayData.filter(d => d.sessions.length === 0);

if (noTimestamp.length > 0) warn(`${noTimestamp.length} session(s) with NO timestamp — invisible in history views`);
else ok("All sessions have timestamps");

if (noData.length > 0) warn(`${noData.length} session(s) with JSON sidecar but NO data file (CSV/Parquet)`);
else ok("All sessions have data files");

if (noMetrics.length > 0) row("Info", `${noMetrics.length} session(s) missing _metrics file`, Y);
else ok("All sessions have metrics files");

if (emptyDays.length > 0) row("Info", `${emptyDays.length} day dir(s) with no sessions (log-only)`, D);

// ── Timezone debugging ───────────────────────────────────────────────────────

section("Timezone Debug");
row("JS getTimezoneOffset()", `${tzOff} minutes (${-tzOff/60}h east of UTC)`);
row("tzOffsetSecs for Rust", String(tzOff * -60));
row("Intl timezone", tzName);

// Check if any UTC dir → local day mapping would lose sessions
if (allSessions.length > 0 && allSessions[0].start) {
  const newest = allSessions[0];
  const oldest = allSessions[allSessions.length - 1];
  row("Newest session", `${fmtTs(newest.start)} (UTC dir ${newest.utcDir}, local ${fmtLocalDay(newest.start)})`);
  row("Oldest session", `${fmtTs(oldest.start)} (UTC dir ${oldest.utcDir}, local ${fmtLocalDay(oldest.start)})`);
}

// ── Global indices ───────────────────────────────────────────────────────────

section("Global Indices & Databases");
const idxFiles = [
  ["eeg_global.hnsw", "EEG embedding index"],
  ["eeg_global_luna.hnsw", "EEG Luna index"],
  ["label_text_index.hnsw", "Label text HNSW"],
  ["label_eeg_index.hnsw", "Label EEG HNSW"],
  ["label_context_index.hnsw", "Label context HNSW"],
  ["screenshots.hnsw", "Screenshot CLIP HNSW"],
  ["screenshots_ocr.hnsw", "Screenshot OCR HNSW"],
];
for (const [f, label] of idxFiles) {
  const fp = join(SKILL_DIR, f);
  if (existsSync(fp)) ok(`${label}: ${fmtB(statSync(fp).size)}`);
  else row(label, "not found", D);
}

const dbs = ["labels.sqlite", "hooks.sqlite", "health.sqlite", "activity.sqlite", "screenshots.sqlite", "chat_history.sqlite"];
for (const db of dbs) {
  const fp = join(SKILL_DIR, db);
  if (existsSync(fp)) ok(`${db}: ${fmtB(statSync(fp).size)}`);
  else row(db, "not found", D);
}

// ── Models ───────────────────────────────────────────────────────────────────

section("Models");
const hfCache = process.env.HUGGINGFACE_HUB_CACHE
  || (process.env.HF_HOME ? join(process.env.HF_HOME, "hub") : join(homedir(), ".cache", "huggingface", "hub"));
if (existsSync(hfCache)) {
  const models = readdirSync(hfCache).filter(f => f.startsWith("models--")).sort();
  row("HF hub cache", hfCache);
  row("Cached models", String(models.length), C);
  for (const m of models) {
    row(`  ${m.replace("models--", "").replace("--", "/")}`, fmtB(dirSize(join(hfCache, m))));
  }
}
const feCache = join(SKILL_DIR, "fastembed_cache");
if (existsSync(feCache)) warn(`Legacy fastembed_cache exists (${fmtB(dirSize(feCache))})`);

const modelCfg = join(SKILL_DIR, "model_config.json");
if (existsSync(modelCfg)) {
  try {
    const cfg = JSON.parse(readFileSync(modelCfg, "utf-8"));
    row("EXG model backend", cfg.model_backend || "?", C);
    row("EXG HF repo", cfg.hf_repo || "?");
    row("EXG embed dim", String(cfg.embed_dim || "?"));
    row("EXG data norm", String(cfg.data_norm || "?"));
    if (cfg.luna_variant) row("Luna variant", cfg.luna_variant);
  } catch {}
}

// ── Screenshots ──────────────────────────────────────────────────────────────

section("Screenshots");
const ssDir = join(SKILL_DIR, "screenshots");
if (existsSync(ssDir)) {
  const ssDays = readdirSync(ssDir, { withFileTypes: true }).filter(e => e.isDirectory());
  let totalSs = 0;
  for (const d of ssDays) totalSs += readdirSync(join(ssDir, d.name)).length;
  row("Screenshot days", String(ssDays.length));
  row("Total screenshots", String(totalSs), C);
  row("Screenshots size", fmtB(dirSize(ssDir)));
} else row("Screenshots", "none", D);

// ── Summary ──────────────────────────────────────────────────────────────────

section("Summary");
row("Total ~/.skill size", fmtB(dirSize(SKILL_DIR)), B);
row("Sessions", `${totSessions} across ${dayData.filter(d=>d.sessions.length>0).length} days`);
row("Local days", `${localDays.length} (from session timestamps)`);
row("Modalities", [
  totCsv + totParquet > 0 ? `EEG:${totCsv + totParquet}` : null,
  totPpg > 0 ? `PPG:${totPpg}` : null,
  totImu > 0 ? `IMU:${totImu}` : null,
].filter(Boolean).join("  ") || "none");

console.log("");
