#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// npm run health — inspect ~/.skill and report on data, databases, indices,
// models, metrics, modalities, and general directory health.

import { readdirSync, statSync, existsSync, readFileSync } from "fs";
import { join, basename, extname } from "path";
import { homedir } from "os";

const SKILL_DIR = process.env.SKILL_DIR || join(homedir(), ".skill");
const BOLD = "\x1b[1m";
const DIM = "\x1b[2m";
const RESET = "\x1b[0m";
const GREEN = "\x1b[32m";
const YELLOW = "\x1b[33m";
const RED = "\x1b[31m";
const CYAN = "\x1b[36m";

// ── Helpers ───────────────────────────────────────────────────────────────────

function fmtBytes(b) {
  if (b >= 1e9) return `${(b / 1e9).toFixed(2)} GB`;
  if (b >= 1e6) return `${(b / 1e6).toFixed(1)} MB`;
  if (b >= 1e3) return `${(b / 1e3).toFixed(1)} KB`;
  return `${b} B`;
}

function dirSize(dir) {
  let total = 0;
  try {
    for (const f of readdirSync(dir, { withFileTypes: true })) {
      const p = join(dir, f.name);
      if (f.isDirectory()) total += dirSize(p);
      else if (f.isFile()) total += statSync(p).size;
    }
  } catch {}
  return total;
}

function countFiles(dir, ext) {
  let n = 0;
  try {
    for (const f of readdirSync(dir, { withFileTypes: true })) {
      if (f.isFile() && f.name.endsWith(ext)) n++;
      else if (f.isDirectory()) n += countFiles(join(dir, f.name), ext);
    }
  } catch {}
  return n;
}

function isDateDir(name) {
  return /^\d{8}$/.test(name);
}

function section(title) {
  console.log(`\n${BOLD}── ${title} ${"─".repeat(Math.max(0, 60 - title.length))}${RESET}`);
}

function row(label, value, color = "") {
  console.log(`  ${DIM}${label.padEnd(32)}${RESET} ${color}${value}${color ? RESET : ""}`);
}

function ok(msg) { row("✓", msg, GREEN); }
function warn(msg) { row("⚠", msg, YELLOW); }
function err(msg) { row("✗", msg, RED); }

// ── Main ──────────────────────────────────────────────────────────────────────

if (!existsSync(SKILL_DIR)) {
  console.log(`${RED}~/.skill not found at ${SKILL_DIR}${RESET}`);
  process.exit(1);
}

console.log(`${BOLD}NeuroSkill™ Health Report${RESET}`);
console.log(`${DIM}${SKILL_DIR}${RESET}`);

// ── Day directories ──────────────────────────────────────────────────────────

section("Recording Days");

const entries = readdirSync(SKILL_DIR, { withFileTypes: true });
const dayDirs = entries.filter(e => e.isDirectory() && isDateDir(e.name)).map(e => e.name).sort();

row("Total day directories", String(dayDirs.length), CYAN);

let totalSessions = 0;
let totalCsv = 0;
let totalParquet = 0;
let totalMetricsCsv = 0;
let totalPpg = 0;
let totalImu = 0;
let totalJsonSidecars = 0;
let totalLogs = 0;
let totalSqlite = 0;
let totalHnsw = 0;
let daysWithSessions = 0;
let totalDataBytes = 0;

const sessionsByDay = [];

for (const day of dayDirs) {
  const dp = join(SKILL_DIR, day);
  const files = readdirSync(dp);
  let daySessions = 0;

  for (const f of files) {
    const ext = extname(f);
    const fp = join(dp, f);
    try { totalDataBytes += statSync(fp).size; } catch {}

    if ((f.startsWith("exg_") || f.startsWith("muse_")) && f.endsWith(".json") && !f.endsWith("_metrics_cache.json")) {
      totalJsonSidecars++;
      daySessions++;
    }
    if (ext === ".csv" && !f.includes("_metrics") && !f.includes("_ppg") && !f.includes("_imu")) totalCsv++;
    if (ext === ".parquet" && !f.includes("_metrics") && !f.includes("_ppg") && !f.includes("_imu")) totalParquet++;
    if (f.includes("_metrics.csv") || f.includes("_metrics.parquet")) totalMetricsCsv++;
    if (f.includes("_ppg.csv") || f.includes("_ppg.parquet")) totalPpg++;
    if (f.includes("_imu.csv") || f.includes("_imu.parquet")) totalImu++;
    if (f.startsWith("log_") && ext === ".txt") totalLogs++;
    if (f.endsWith(".sqlite") || f.endsWith(".sqlite-wal") || f.endsWith(".sqlite-shm")) totalSqlite++;
    if (f.endsWith(".hnsw")) totalHnsw++;
  }

  totalSessions += daySessions;
  if (daySessions > 0) daysWithSessions++;
  sessionsByDay.push({ day, sessions: daySessions });
}

row("Days with sessions", `${daysWithSessions} / ${dayDirs.length}`, daysWithSessions > 0 ? GREEN : RED);
row("Total sessions (JSON sidecars)", String(totalSessions), CYAN);
row("Date range", dayDirs.length > 0 ? `${dayDirs[0]} → ${dayDirs[dayDirs.length - 1]}` : "none");
row("Recording data size", fmtBytes(totalDataBytes));

// ── Data Files ───────────────────────────────────────────────────────────────

section("Data Files");
row("EEG CSV files", String(totalCsv));
row("EEG Parquet files", String(totalParquet));
row("Metrics files", String(totalMetricsCsv));
row("PPG files", String(totalPpg));
row("IMU files", String(totalImu));
row("Log files", String(totalLogs));
row("SQLite databases", String(totalSqlite));
row("HNSW indices", String(totalHnsw));

// ── Modality coverage ────────────────────────────────────────────────────────

section("Modalities");
if (totalCsv + totalParquet > 0) ok(`EEG: ${totalCsv + totalParquet} files`);
else warn("EEG: no data files found");
if (totalPpg > 0) ok(`PPG (heart rate): ${totalPpg} files`);
else row("PPG", "none", DIM);
if (totalImu > 0) ok(`IMU (motion): ${totalImu} files`);
else row("IMU", "none", DIM);

// ── Global indices ───────────────────────────────────────────────────────────

section("Global Indices");
const globalFiles = [
  ["eeg_global.hnsw", "EEG embedding index"],
  ["eeg_global_luna.hnsw", "EEG Luna embedding index"],
  ["label_text_index.hnsw", "Label text HNSW"],
  ["label_eeg_index.hnsw", "Label EEG HNSW"],
  ["label_context_index.hnsw", "Label context HNSW"],
  ["screenshots.hnsw", "Screenshot CLIP HNSW"],
  ["screenshots_ocr.hnsw", "Screenshot OCR HNSW"],
];
for (const [file, label] of globalFiles) {
  const fp = join(SKILL_DIR, file);
  if (existsSync(fp)) {
    const sz = statSync(fp).size;
    ok(`${label}: ${fmtBytes(sz)}`);
  } else {
    row(label, "not found", DIM);
  }
}

// ── Databases ────────────────────────────────────────────────────────────────

section("Databases");
const dbs = ["labels.sqlite", "hooks.sqlite", "health.sqlite", "activity.sqlite",
             "screenshots.sqlite", "chat_history.sqlite"];
for (const db of dbs) {
  const fp = join(SKILL_DIR, db);
  if (existsSync(fp)) {
    ok(`${db}: ${fmtBytes(statSync(fp).size)}`);
  } else {
    row(db, "not found", DIM);
  }
}
// Chat DB (alternate location)
const chatDb = join(SKILL_DIR, "chats", "chat_history.sqlite");
if (existsSync(chatDb)) ok(`chats/chat_history.sqlite: ${fmtBytes(statSync(chatDb).size)}`);

// ── Models ───────────────────────────────────────────────────────────────────

section("Models & Caches");

// HuggingFace hub cache
const hfCache = process.env.HUGGINGFACE_HUB_CACHE
  || (process.env.HF_HOME ? join(process.env.HF_HOME, "hub") : join(homedir(), ".cache", "huggingface", "hub"));
if (existsSync(hfCache)) {
  const models = readdirSync(hfCache).filter(f => f.startsWith("models--"));
  row("HF hub cache", hfCache);
  row("Cached models", String(models.length), CYAN);
  for (const m of models.sort()) {
    const sz = dirSize(join(hfCache, m));
    row(`  ${m.replace("models--", "").replace("--", "/")}`, fmtBytes(sz));
  }
} else {
  row("HF hub cache", "not found", DIM);
}

// Legacy fastembed_cache (should be migrated)
const feCache = join(SKILL_DIR, "fastembed_cache");
if (existsSync(feCache)) {
  warn(`Legacy fastembed_cache still exists (${fmtBytes(dirSize(feCache))}) — will be migrated on next app launch`);
}

// LLM catalog
const llmCat = join(SKILL_DIR, "llm_catalog.json");
if (existsSync(llmCat)) ok(`LLM catalog: ${fmtBytes(statSync(llmCat).size)}`);

// Model config
const modelCfg = join(SKILL_DIR, "model_config.json");
if (existsSync(modelCfg)) {
  try {
    const cfg = JSON.parse(readFileSync(modelCfg, "utf-8"));
    row("EXG model backend", cfg.model_backend || "unknown", CYAN);
    row("EXG HF repo", cfg.hf_repo || "unknown");
  } catch { ok("model_config.json exists"); }
}

// NeuTTS models
const neuttsDir = join(SKILL_DIR, "models", "neutts");
if (existsSync(neuttsDir)) {
  ok(`NeuTTS models: ${fmtBytes(dirSize(neuttsDir))}`);
}

// OCR models
const ocrDir = join(SKILL_DIR, "ocr_models");
if (existsSync(ocrDir)) {
  const ocrFiles = readdirSync(ocrDir);
  ok(`OCR models: ${ocrFiles.length} file(s), ${fmtBytes(dirSize(ocrDir))}`);
}

// ── Screenshots ──────────────────────────────────────────────────────────────

section("Screenshots");
const ssDir = join(SKILL_DIR, "screenshots");
if (existsSync(ssDir)) {
  const ssDays = readdirSync(ssDir, { withFileTypes: true }).filter(e => e.isDirectory());
  let totalSs = 0;
  for (const d of ssDays) totalSs += readdirSync(join(ssDir, d.name)).length;
  row("Screenshot days", String(ssDays.length));
  row("Total screenshots", String(totalSs), CYAN);
  row("Screenshots size", fmtBytes(dirSize(ssDir)));
} else {
  row("Screenshots", "none", DIM);
}

// ── LLM logs ─────────────────────────────────────────────────────────────────

section("LLM Logs");
const llmLogDir = join(SKILL_DIR, "llm_logs");
if (existsSync(llmLogDir)) {
  const llmLogs = readdirSync(llmLogDir).filter(f => f.endsWith(".txt"));
  row("LLM log files", String(llmLogs.length));
  row("LLM logs size", fmtBytes(dirSize(llmLogDir)));
} else {
  row("LLM logs", "none", DIM);
}

// ── UMAP cache ───────────────────────────────────────────────────────────────

const umapDir = join(SKILL_DIR, "umap_cache");
if (existsSync(umapDir)) {
  const umapFiles = readdirSync(umapDir).filter(f => f.endsWith(".json"));
  row("UMAP cache files", String(umapFiles.length));
}

// ── Settings ─────────────────────────────────────────────────────────────────

section("Settings");
const settingsFile = join(SKILL_DIR, "settings.json");
if (existsSync(settingsFile)) ok(`settings.json: ${fmtBytes(statSync(settingsFile).size)}`);
const logCfg = join(SKILL_DIR, "log_config.json");
if (existsSync(logCfg)) ok("log_config.json");
const irohAuth = join(SKILL_DIR, "iroh_auth.json");
if (existsSync(irohAuth)) ok("iroh_auth.json (sync configured)");

// ── Total ────────────────────────────────────────────────────────────────────

section("Summary");
const totalSize = dirSize(SKILL_DIR);
row("Total ~/.skill size", fmtBytes(totalSize), BOLD);
row("Sessions", String(totalSessions));
row("Recording days", `${daysWithSessions} with data, ${dayDirs.length} total`);

console.log("");
