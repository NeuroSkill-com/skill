#!/usr/bin/env node

import { existsSync, lstatSync, readdirSync, readFileSync } from "node:fs";
import { resolve, dirname, relative } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");
const tauriDir = resolve(root, "src-tauri");
const tauriConfPath = resolve(tauriDir, "tauri.conf.json");

function fail(message) {
  console.error(`✖ ${message}`);
  process.exit(1);
}

function parseJsonFile(path, label) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    fail(`Unable to parse ${label} at ${path}: ${error.message}`);
  }
}

function isInside(parentPath, childPath) {
  const rel = relative(parentPath, childPath);
  return rel && !rel.startsWith("..") && !rel.startsWith("/");
}

function findFrontendDistPath(tauriConf) {
  const buildCfg = tauriConf?.build ?? {};
  const frontendDist = buildCfg.frontendDist ?? buildCfg.distDir;

  if (!frontendDist || typeof frontendDist !== "string") {
    fail(
      "src-tauri/tauri.conf.json must define build.frontendDist (or legacy build.distDir)."
    );
  }

  return resolve(tauriDir, frontendDist);
}

function verifyFrontendDistShape(frontendDistPath) {
  if (!existsSync(frontendDistPath)) {
    fail(
      `Configured frontendDist does not exist: ${frontendDistPath}. Run npm run build first.`
    );
  }

  if (!lstatSync(frontendDistPath).isDirectory()) {
    fail(`Configured frontendDist is not a directory: ${frontendDistPath}`);
  }

  const indexHtml = resolve(frontendDistPath, "index.html");
  if (!existsSync(indexHtml)) {
    fail(
      `Missing ${indexHtml}. Tauri expects built frontend assets, not raw src files.`
    );
  }

  const appDir = resolve(frontendDistPath, "_app");
  if (!existsSync(appDir) || !lstatSync(appDir).isDirectory()) {
    fail(
      `Missing ${appDir}. Expected SvelteKit static output with bundled assets.`
    );
  }

  const immutableDir = resolve(appDir, "immutable");
  if (!existsSync(immutableDir) || !lstatSync(immutableDir).isDirectory()) {
    fail(`Missing ${immutableDir}. Expected compiled JS/CSS assets for Tauri bundling.`);
  }

  const jsAndCssCount = readdirSync(immutableDir, { recursive: true })
    .map((entry) => String(entry).toLowerCase())
    .filter((name) => name.endsWith(".js") || name.endsWith(".css")).length;

  if (jsAndCssCount === 0) {
    fail(
      `No compiled .js/.css assets found under ${immutableDir}. Frontend build output looks incomplete.`
    );
  }
}

const tauriConf = parseJsonFile(tauriConfPath, "tauri.conf.json");
const packageJson = parseJsonFile(resolve(root, "package.json"), "package.json");

const frontendDistPath = findFrontendDistPath(tauriConf);

if (!isInside(root, frontendDistPath)) {
  fail(`frontendDist must resolve inside repository root: ${frontendDistPath}`);
}

if (!isInside(root, tauriDir)) {
  fail("Internal path sanity check failed for src-tauri/.");
}

if (frontendDistPath === resolve(root, "src") || frontendDistPath.endsWith("/src")) {
  fail("frontendDist cannot point to raw src/; it must point to built assets output.");
}

const buildScript = packageJson?.scripts?.build;
if (typeof buildScript !== "string" || !buildScript.includes("vite build")) {
  fail("package.json scripts.build must include `vite build` so Tauri bundles compiled assets.");
}

verifyFrontendDistShape(frontendDistPath);

console.log("✓ Tauri frontend bundle structure verified.");
console.log(`  frontendDist: ${frontendDistPath}`);
