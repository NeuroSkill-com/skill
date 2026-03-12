#!/usr/bin/env node

import fs from 'node:fs/promises';
import path from 'node:path';

const ROOT = process.cwd();
const CATALOG_PATH = path.join(ROOT, 'src-tauri', 'llm_catalog.json');
const HUGGING_FACE_API = 'https://huggingface.co/api/models/';

const args = new Set(process.argv.slice(2));
const writeMode = args.has('--write');
const checkMode = args.has('--check') || !writeMode;
const verbose = args.has('--verbose');

function log(...msg) {
  console.log('[sync-llm-catalog]', ...msg);
}

function normalizeQuant(value) {
  return value ? value.toUpperCase() : 'UNKNOWN';
}

function inferQuant(filename, isMmproj) {
  const stem = filename.replace(/\.gguf$/i, '');

  if (isMmproj) {
    const mmprojMatch = stem.match(/-(bf16|f16|f32)$/i);
    return mmprojMatch ? normalizeQuant(mmprojMatch[1]) : 'MMPROJ';
  }

  const quantMatch = stem.match(/-((?:IQ|Q)[A-Za-z0-9_]+|BF16|F16|F32)$/i);
  return quantMatch ? normalizeQuant(quantMatch[1]) : 'UNKNOWN';
}

function inferDescription({ quant, isMmproj }) {
  if (isMmproj) {
    if (quant === 'BF16') return 'Vision projector — BF16 (recommended)';
    if (quant === 'F16') return 'Vision projector — FP16';
    if (quant === 'F32') return 'Vision projector — FP32';
    return 'Vision projector';
  }

  if (quant === 'Q4_K_M') return 'Recommended — best quality/size tradeoff';
  if (quant === 'Q4_0') return 'Legacy 4-bit quant';
  if (quant === 'Q2_K') return 'Ultra-compressed; lowest quality';
  if (quant === 'Q6_K') return 'Near-lossless quality';
  if (quant === 'Q8_0') return 'Effectively lossless 8-bit';
  if (quant === 'F16' || quant === 'BF16') return 'Full precision weights';
  return `${quant} quant`;
}

function inferAdvanced({ quant, isMmproj, recommended }) {
  if (isMmproj) return quant === 'F32';
  if (recommended) return false;
  return !['Q4_0', 'Q4_1', 'Q4_K_M', 'Q4_K_S', 'Q4_K_L'].includes(quant);
}

function inferRecommended({ quant, isMmproj }) {
  if (isMmproj) return quant === 'BF16';
  return quant === 'Q4_K_M';
}

function quantSortKey(quant, isMmproj) {
  const mmprojOrder = ['BF16', 'F16', 'F32'];
  const quantOrder = [
    'IQ1_S', 'IQ1_M',
    'IQ2_XXS', 'IQ2_XS', 'IQ2_S', 'IQ2_M',
    'Q2_K', 'Q2_K_L',
    'IQ3_XXS', 'IQ3_XS', 'Q3_K_S', 'IQ3_M', 'Q3_K_M', 'Q3_K_L', 'Q3_K_XL',
    'IQ4_XS', 'IQ4_NL', 'Q4_0', 'Q4_1', 'Q4_K_S', 'Q4_K_M', 'Q4_K_L',
    'Q5_K_S', 'Q5_K_M', 'Q5_K_L',
    'Q6_K', 'Q6_K_L',
    'Q8_0',
    'F16', 'BF16', 'F32'
  ];

  const order = isMmproj ? mmprojOrder : quantOrder;
  const idx = order.indexOf(quant);
  return idx === -1 ? Number.MAX_SAFE_INTEGER : idx;
}

async function fetchRepoSiblings(repo) {
  const url = new URL(`${HUGGING_FACE_API}${repo}`);
  url.searchParams.append('expand[]', 'siblings');

  const response = await fetch(url, {
    headers: {
      accept: 'application/json',
      'user-agent': 'skill-sync-llm-catalog/1.0'
    }
  });

  if (!response.ok) {
    throw new Error(`HTTP ${response.status} for ${repo}`);
  }

  const json = await response.json();
  return Array.isArray(json?.siblings) ? json.siblings : [];
}

async function loadCatalog() {
  const raw = await fs.readFile(CATALOG_PATH, 'utf8');
  return JSON.parse(raw);
}

function createAddedEntry(filename, siblingMap, template) {
  const isMmproj = /mmproj/i.test(filename);
  const quant = inferQuant(filename, isMmproj);
  const sibling = siblingMap.get(filename);
  const sizeBytes = typeof sibling?.size === 'number' ? sibling.size : undefined;
  const sizeGb = sizeBytes ? Number((sizeBytes / (1024 ** 3)).toFixed(2)) : template.size_gb;
  const recommended = inferRecommended({ quant, isMmproj });

  return {
    repo: template.repo,
    filename,
    quant,
    size_gb: sizeGb,
    description: inferDescription({ quant, isMmproj }),
    family_id: template.family_id,
    family_name: isMmproj ? '' : template.family_name,
    family_desc: isMmproj ? '' : template.family_desc,
    tags: isMmproj ? ['vision', 'multimodal'] : template.tags,
    is_mmproj: isMmproj,
    recommended,
    advanced: inferAdvanced({ quant, isMmproj, recommended })
  };
}

function uniqueRepoOrder(entries) {
  const seen = new Set();
  const ordered = [];
  for (const entry of entries) {
    if (!seen.has(entry.repo)) {
      seen.add(entry.repo);
      ordered.push(entry.repo);
    }
  }
  return ordered;
}

function pruneMmprojOnlyFamilies(entries) {
  const textFamilies = new Set(
    entries
      .filter((entry) => !entry.is_mmproj)
      .map((entry) => entry.family_id)
  );

  return entries.filter((entry) => !entry.is_mmproj || textFamilies.has(entry.family_id));
}

async function syncCatalog() {
  const catalog = await loadCatalog();
  const originalEntries = catalog.entries;

  const repos = uniqueRepoOrder(originalEntries);
  const repoEntryMap = new Map();
  for (const repo of repos) {
    repoEntryMap.set(repo, originalEntries.filter((entry) => entry.repo === repo));
  }

  const removedKeys = new Set();
  const additionsByRepo = new Map();
  const stats = {
    checkedRepos: 0,
    removed: 0,
    added: 0,
    failedRepos: []
  };

  for (const repo of repos) {
    const existing = repoEntryMap.get(repo) ?? [];
    if (existing.length === 0) continue;

    stats.checkedRepos += 1;

    let siblings;
    try {
      siblings = await fetchRepoSiblings(repo);
    } catch (error) {
      stats.failedRepos.push({ repo, error: String(error) });
      log(`warn: failed to fetch ${repo}: ${error}`);
      continue;
    }

    const siblingMap = new Map(siblings.map((s) => [s.rfilename, s]));
    const remoteGguf = new Set(
      siblings
        .map((s) => s.rfilename)
        .filter((name) => typeof name === 'string')
        .filter((name) => name.toLowerCase().endsWith('.gguf'))
        .filter((name) => !name.includes('/'))
        .filter((name) => !name.toLowerCase().includes('imatrix'))
    );

    const existingFilenames = new Set(existing.map((entry) => entry.filename));

    for (const entry of existing) {
      if (!remoteGguf.has(entry.filename)) {
        removedKeys.add(`${entry.repo}::${entry.filename}`);
      }
    }

    const templateModel = existing.find((entry) => !entry.is_mmproj) ?? existing[0];

    const newEntries = [];
    for (const filename of remoteGguf) {
      if (existingFilenames.has(filename)) continue;
      newEntries.push(createAddedEntry(filename, siblingMap, templateModel));
    }

    newEntries.sort((a, b) => {
      if (a.is_mmproj !== b.is_mmproj) return a.is_mmproj ? 1 : -1;
      const qa = quantSortKey(a.quant, a.is_mmproj);
      const qb = quantSortKey(b.quant, b.is_mmproj);
      if (qa !== qb) return qa - qb;
      return a.filename.localeCompare(b.filename);
    });

    additionsByRepo.set(repo, newEntries);
  }

  const mergedEntries = [];
  for (const repo of repos) {
    const existing = repoEntryMap.get(repo) ?? [];
    for (const entry of existing) {
      const key = `${entry.repo}::${entry.filename}`;
      if (!removedKeys.has(key)) {
        mergedEntries.push(entry);
      } else {
        stats.removed += 1;
        if (verbose) log(`remove ${entry.repo}/${entry.filename}`);
      }
    }

    const additions = additionsByRepo.get(repo) ?? [];
    for (const entry of additions) {
      mergedEntries.push(entry);
      stats.added += 1;
      if (verbose) log(`add ${entry.repo}/${entry.filename}`);
    }
  }

  const prunedEntries = pruneMmprojOnlyFamilies(mergedEntries);
  stats.removed += mergedEntries.length - prunedEntries.length;

  const nextCatalog = { ...catalog, entries: prunedEntries };
  const changed = JSON.stringify(nextCatalog) !== JSON.stringify(catalog);

  return { changed, nextCatalog, stats };
}

async function main() {
  const { changed, nextCatalog, stats } = await syncCatalog();

  log(`checked repos: ${stats.checkedRepos}`);
  log(`added: ${stats.added}, removed: ${stats.removed}`);

  if (stats.failedRepos.length > 0) {
    log(`repos with fetch errors: ${stats.failedRepos.length}`);
    for (const item of stats.failedRepos) {
      log(`  - ${item.repo}: ${item.error}`);
    }
  }

  if (!changed) {
    log('catalog is up to date');
    process.exit(0);
  }

  if (checkMode && !writeMode) {
    log('catalog needs updates (run with --write to apply changes)');
    process.exit(1);
  }

  await fs.writeFile(CATALOG_PATH, `${JSON.stringify(nextCatalog, null, 2)}\n`, 'utf8');
  log('catalog updated');
}

main().catch((error) => {
  console.error('[sync-llm-catalog] fatal:', error);
  process.exit(1);
});
