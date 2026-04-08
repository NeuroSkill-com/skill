#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only

import fs from "node:fs";
import path from "node:path";

const ROOT = process.cwd();
const README_PATH = path.join(ROOT, "README.md");
const DEVICE_RS_PATH = path.join(ROOT, "crates/skill-data/src/device.rs");
const EN_I18N_PATH = path.join(ROOT, "src/lib/i18n/en/settings.ts");
const EXG_CATALOG_PATH = path.join(ROOT, "src-tauri/exg_catalog.json");

const MODE = process.argv.includes("--check") ? "check" : "write";

function readUtf8(p) {
  return fs.readFileSync(p, "utf8");
}

function parseI18nMap(src) {
  const map = new Map();
  const re = /"([^"]+)"\s*:\s*"((?:\\.|[^"])*)",/g;
  let m = re.exec(src);
  while (m) {
    const key = m[1];
    const rawVal = m[2];
    const val = JSON.parse(`"${rawVal.replace(/"/g, '\\"')}"`);
    map.set(key, val);
    m = re.exec(src);
  }
  return map;
}

function extractStructBlocks(src, marker) {
  const out = [];
  let idx = 0;
  while (true) {
    const start = src.indexOf(marker, idx);
    if (start === -1) break;

    const open = src.indexOf("{", start);
    if (open === -1) break;

    let depth = 0;
    let end = -1;
    for (let i = open; i < src.length; i++) {
      const ch = src[i];
      if (ch === "{") depth++;
      if (ch === "}") {
        depth--;
        if (depth === 0) {
          end = i;
          break;
        }
      }
    }

    if (end !== -1) {
      out.push(src.slice(open + 1, end));
      idx = end + 1;
    } else {
      break;
    }
  }
  return out;
}

function supportedCompaniesSourceSlice(deviceRs) {
  const fnIdx = deviceRs.indexOf("pub fn supported_companies()");
  if (fnIdx === -1) return deviceRs;
  const vecIdx = deviceRs.indexOf("vec![", fnIdx);
  if (vecIdx === -1) return deviceRs;

  const open = deviceRs.indexOf("[", vecIdx);
  if (open === -1) return deviceRs;

  let depth = 0;
  let end = -1;
  for (let i = open; i < deviceRs.length; i++) {
    const ch = deviceRs[i];
    if (ch === "[") depth++;
    if (ch === "]") {
      depth--;
      if (depth === 0) {
        end = i;
        break;
      }
    }
  }

  if (end === -1) return deviceRs;
  return deviceRs.slice(open + 1, end);
}

function parseSupportedCompanies(deviceRs, i18nMap) {
  const catalogSrc = supportedCompaniesSourceSlice(deviceRs);
  const blocks = extractStructBlocks(catalogSrc, "SupportedCompany");
  const out = [];

  for (const block of blocks) {
    const companyKeyMatch = block.match(/name_key:\s*"(settings\.supportedDevices\.company\.[^"]+)"/);
    if (!companyKeyMatch) continue;

    const companyKey = companyKeyMatch[1];
    const companyName = i18nMap.get(companyKey) ?? companyKey;

    const devices = [];
    for (const devMatch of block.matchAll(/SupportedDevice\s*\{([\s\S]*?)\}/g)) {
      const devBlock = devMatch[1];
      const devKeyMatch = devBlock.match(/name_key:\s*"(settings\.supportedDevices\.device\.[^"]+)"/);
      if (!devKeyMatch) continue;
      const devKey = devKeyMatch[1];
      const devName = i18nMap.get(devKey) ?? devKey;
      const iosOnly = /ios_only:\s*true/.test(devBlock);
      devices.push({ name: devName, iosOnly });
    }

    if (devices.length > 0) {
      out.push({ companyName, devices });
    }
  }

  return out;
}

function buildDeviceSection(companies) {
  const lines = [];
  for (const c of companies) {
    const hasIosOnly = c.devices.some((d) => d.iosOnly);
    const names = c.devices.map((d) => d.name).join(", ");
    const iosSuffix = hasIosOnly ? " *(iOS bridge only)*" : "";
    lines.push(`- **${c.companyName}:** ${names}${iosSuffix}`);
  }
  lines.push("");
  lines.push("Plus any compatible **LSL** source (e.g. BrainFlow, MATLAB, pylsl).");
  return lines.join("\n");
}

function buildModelsSection(exgCatalog) {
  const lines = [];
  for (const family of Object.values(exgCatalog.families)) {
    const name = family.name;
    const repo = family.repo;
    lines.push(`- **${name}** (\`${repo}\`)`);
  }
  return lines.join("\n");
}

function replaceBetween(content, startMarker, endMarker, replacementInner) {
  const escapedStart = startMarker.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const escapedEnd = endMarker.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`${escapedStart}[\\s\\S]*?${escapedEnd}`);
  const match = content.match(re);
  if (!match) {
    throw new Error(`Could not find marker block: ${startMarker} ... ${endMarker}`);
  }
  return content.replace(re, `${startMarker}\n${replacementInner}\n${endMarker}`);
}

function main() {
  const readme = readUtf8(README_PATH);
  const deviceRs = readUtf8(DEVICE_RS_PATH);
  const en = readUtf8(EN_I18N_PATH);
  const exgCatalog = JSON.parse(readUtf8(EXG_CATALOG_PATH));

  const i18nMap = parseI18nMap(en);
  const companies = parseSupportedCompanies(deviceRs, i18nMap);

  const devicesBlock = ["<!-- Run: npm run sync:readme:supported -->", buildDeviceSection(companies)].join("\n");

  const modelsBlock = ["<!-- Run: npm run sync:readme:supported -->", buildModelsSection(exgCatalog)].join("\n");

  let next = replaceBetween(
    readme,
    "<!-- AUTO-GENERATED:SUPPORTED_DEVICES:START -->",
    "<!-- AUTO-GENERATED:SUPPORTED_DEVICES:END -->",
    devicesBlock,
  );

  next = replaceBetween(
    next,
    "<!-- AUTO-GENERATED:SUPPORTED_MODELS:START -->",
    "<!-- AUTO-GENERATED:SUPPORTED_MODELS:END -->",
    modelsBlock,
  );

  if (MODE === "check") {
    if (next !== readme) {
      console.error("README supported sections are out of date. Run: npm run sync:readme:supported");
      process.exit(1);
    }
    console.log("README supported sections are up to date.");
    return;
  }

  fs.writeFileSync(README_PATH, next, "utf8");
  console.log("Updated README supported devices/models sections.");
}

main();
