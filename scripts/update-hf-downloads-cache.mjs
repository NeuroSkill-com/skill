#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Build / refresh the HuggingFace downloads cache used to sort the LLM
// catalog (and surface mlx-community popularity) offline.
//
// Sources:
//   1. Every unique `repo` in src-tauri/llm_catalog.json (GGUF + mlx packs)
//   2. A crawl of huggingface.co/mlx-community sorted by downloads
//
// Output (committed): src-tauri/hf_downloads_cache.json
//
//   node scripts/update-hf-downloads-cache.mjs --write
//   node scripts/update-hf-downloads-cache.mjs --check
//   node scripts/update-hf-downloads-cache.mjs --write --verbose
//
// Env: HF_TOKEN / HUGGING_FACE_HUB_TOKEN (optional, higher rate limits)
//      HF_ENDPOINT (default https://huggingface.co)

import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const CATALOG_PATH = path.join(ROOT, "src-tauri", "llm_catalog.json");
const CACHE_PATH = path.join(ROOT, "src-tauri", "hf_downloads_cache.json");
/** Frontend import path — keep in sync with CACHE_PATH on every --write. */
const GENERATED_CACHE_PATH = path.join(ROOT, "src", "lib", "generated", "hf-downloads-cache.json");

const args = new Set(process.argv.slice(2));
const writeMode = args.has("--write");
const checkMode = args.has("--check") || !writeMode;
const verbose = args.has("--verbose");

const HF_ENDPOINT = (process.env.HF_ENDPOINT || "https://huggingface.co").replace(/\/$/, "");
const HF_TOKEN =
  process.env.HF_TOKEN || process.env.HUGGING_FACE_HUB_TOKEN || "";

/** Concurrent Hub GETs — keep low to stay polite without a token. */
const CONCURRENCY = Number(process.env.HF_DOWNLOADS_CONCURRENCY || (HF_TOKEN ? 8 : 4));
/** How many mlx-community models to pull into the popularity crawl. */
const MLX_TOP = Number(process.env.HF_DOWNLOADS_MLX_TOP || 100);
/** Max age (days) before --check warns that the cache is stale. */
const STALE_DAYS = Number(process.env.HF_DOWNLOADS_STALE_DAYS || 14);

function log(...msg) {
  if (verbose) console.log(...msg);
}

function authHeaders() {
  const headers = {
    accept: "application/json",
    "user-agent": "neuroskill-hf-downloads-cache/1.0",
  };
  if (HF_TOKEN) headers.authorization = `Bearer ${HF_TOKEN}`;
  return headers;
}

async function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function fetchJson(url, { retries = 3 } = {}) {
  let lastErr;
  for (let attempt = 0; attempt < retries; attempt++) {
    try {
      const res = await fetch(url, { headers: authHeaders() });
      if (res.status === 429 || res.status >= 500) {
        const wait = 500 * 2 ** attempt;
        log(`retry ${res.status} ${url} in ${wait}ms`);
        await sleep(wait);
        continue;
      }
      if (!res.ok) {
        throw new Error(`HTTP ${res.status} for ${url}`);
      }
      return await res.json();
    } catch (err) {
      lastErr = err;
      if (attempt + 1 < retries) {
        await sleep(400 * 2 ** attempt);
        continue;
      }
    }
  }
  throw lastErr ?? new Error(`fetch failed for ${url}`);
}

/** Collect unique HF repos from the normalized catalog. */
async function catalogRepos() {
  const raw = JSON.parse(await fs.readFile(CATALOG_PATH, "utf8"));
  const repos = new Set();
  for (const fam of Object.values(raw.families ?? {})) {
    if (fam?.repo) repos.add(fam.repo);
  }
  for (const m of raw.models ?? []) {
    if (m?.repo) repos.add(m.repo);
  }
  for (const e of raw.entries ?? []) {
    if (e?.repo) repos.add(e.repo);
  }
  return [...repos].filter(Boolean).sort();
}

async function fetchRepoMeta(repo) {
  const url = `${HF_ENDPOINT}/api/models/${encodeURIComponent(repo).replace(/%2F/g, "/")}`;
  const json = await fetchJson(url);
  return {
    downloads: Number(json.downloads) || 0,
    likes: Number(json.likes) || 0,
    author: typeof json.author === "string" ? json.author : repo.split("/")[0] ?? "",
  };
}

async function mapPool(items, concurrency, fn) {
  const results = new Array(items.length);
  let next = 0;
  async function worker() {
    while (next < items.length) {
      const i = next++;
      results[i] = await fn(items[i], i);
    }
  }
  await Promise.all(Array.from({ length: Math.min(concurrency, items.length) }, () => worker()));
  return results;
}

/** Crawl mlx-community models sorted by Hub downloads (desc). */
async function crawlMlxCommunity(limit) {
  const url = new URL(`${HF_ENDPOINT}/api/models`);
  url.searchParams.set("author", "mlx-community");
  url.searchParams.set("sort", "downloads");
  url.searchParams.set("direction", "-1");
  url.searchParams.set("limit", String(limit));
  const models = await fetchJson(url.toString());
  if (!Array.isArray(models)) return [];
  return models
    .map((m) => ({
      repo: m.id,
      downloads: Number(m.downloads) || 0,
      likes: Number(m.likes) || 0,
    }))
    .filter((m) => typeof m.repo === "string" && m.repo.includes("/"));
}

async function buildCache() {
  const repos = await catalogRepos();
  console.log(`hf-downloads-cache: fetching ${repos.length} catalog repos (concurrency=${CONCURRENCY})…`);

  const failures = [];
  const repoEntries = await mapPool(repos, CONCURRENCY, async (repo) => {
    try {
      const meta = await fetchRepoMeta(repo);
      log(`  ${repo}: ${meta.downloads} downloads`);
      // Be polite between bursts when unauthenticated.
      if (!HF_TOKEN) await sleep(50);
      return [repo, meta];
    } catch (err) {
      failures.push({ repo, error: String(err) });
      console.warn(`warn: ${repo}: ${err}`);
      return null;
    }
  });

  const repoMap = {};
  for (const row of repoEntries) {
    if (row) repoMap[row[0]] = row[1];
  }

  console.log(`hf-downloads-cache: crawling top ${MLX_TOP} mlx-community models by downloads…`);
  let mlxTop = [];
  try {
    mlxTop = await crawlMlxCommunity(MLX_TOP);
    for (const row of mlxTop) {
      // Don't overwrite a fresher per-repo fetch unless missing.
      if (!repoMap[row.repo]) {
        repoMap[row.repo] = {
          downloads: row.downloads,
          likes: row.likes,
          author: "mlx-community",
        };
      }
    }
    log(`  mlx-community top: ${mlxTop.length} models`);
  } catch (err) {
    console.warn(`warn: mlx-community crawl failed: ${err}`);
  }

  const cache = {
    updated_at: new Date().toISOString(),
    source: "huggingface",
    endpoint: HF_ENDPOINT,
    catalog_repos: repos.length,
    fetched_repos: Object.keys(repoMap).length,
    failures: failures.map((f) => f.repo),
    repos: Object.fromEntries(
      Object.entries(repoMap).sort((a, b) => a[0].localeCompare(b[0])),
    ),
    mlx_community_top: mlxTop.sort((a, b) => b.downloads - a.downloads),
  };

  return { cache, failures };
}

function stableStringify(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

async function writeCacheFiles(cache) {
  const body = stableStringify(cache);
  await fs.mkdir(path.dirname(GENERATED_CACHE_PATH), { recursive: true });
  await fs.writeFile(CACHE_PATH, body, "utf8");
  await fs.writeFile(GENERATED_CACHE_PATH, body, "utf8");
}

async function loadExistingCache() {
  try {
    return JSON.parse(await fs.readFile(CACHE_PATH, "utf8"));
  } catch {
    try {
      return JSON.parse(await fs.readFile(GENERATED_CACHE_PATH, "utf8"));
    } catch {
      return null;
    }
  }
}

async function checkCache() {
  const existing = await loadExistingCache();
  if (!existing) {
    console.error(`hf-downloads-cache: missing ${path.relative(ROOT, CACHE_PATH)}`);
    console.error("  run: npm run sync:hf:downloads");
    process.exitCode = 1;
    return;
  }

  // Keep the frontend mirror in lockstep with the canonical src-tauri copy.
  try {
    const gen = await fs.readFile(GENERATED_CACHE_PATH, "utf8");
    const canon = await fs.readFile(CACHE_PATH, "utf8");
    if (gen !== canon) {
      console.error(
        "hf-downloads-cache: src/lib/generated/hf-downloads-cache.json is out of sync with " +
          "src-tauri/hf_downloads_cache.json — run: npm run sync:hf:downloads",
      );
      process.exitCode = 1;
    }
  } catch {
    console.error(
      `hf-downloads-cache: missing ${path.relative(ROOT, GENERATED_CACHE_PATH)} — run: npm run sync:hf:downloads`,
    );
    process.exitCode = 1;
  }

  const repos = await catalogRepos();
  const missing = repos.filter((r) => !existing.repos?.[r]);
  const updatedAt = existing.updated_at ? Date.parse(existing.updated_at) : NaN;
  const ageDays = Number.isFinite(updatedAt)
    ? (Date.now() - updatedAt) / (1000 * 60 * 60 * 24)
    : Infinity;

  console.log(
    `hf-downloads-cache: ${Object.keys(existing.repos ?? {}).length} repos, ` +
      `updated ${existing.updated_at ?? "unknown"} (${ageDays.toFixed(1)}d ago)`,
  );

  if (missing.length) {
    console.error(
      `hf-downloads-cache: ${missing.length} catalog repo(s) missing from cache:\n` +
        missing.map((r) => `  - ${r}`).join("\n"),
    );
    console.error("  run: npm run sync:hf:downloads");
    process.exitCode = 1;
  }

  if (ageDays > STALE_DAYS) {
    console.warn(
      `hf-downloads-cache: stale (>${STALE_DAYS}d). Consider refreshing with npm run sync:hf:downloads`,
    );
  }

  if (!process.exitCode) {
    console.log("hf-downloads-cache: ok");
  }
}

async function main() {
  if (writeMode) {
    const { cache, failures } = await buildCache();
    await writeCacheFiles(cache);
    console.log(
      `hf-downloads-cache: wrote ${path.relative(ROOT, CACHE_PATH)} ` +
        `+ ${path.relative(ROOT, GENERATED_CACHE_PATH)} ` +
        `(${cache.fetched_repos} repos, ${cache.mlx_community_top.length} mlx top` +
        (failures.length ? `, ${failures.length} failures` : "") +
        `)`,
    );
    if (failures.length && failures.length === cache.catalog_repos) {
      process.exitCode = 1;
    }
    return;
  }

  if (checkMode) {
    await checkCache();
  }
}

main().catch((err) => {
  console.error(`hf-downloads-cache: ${err?.stack ?? err}`);
  process.exitCode = 1;
});
