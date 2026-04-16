#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
//
// Shared CI helpers — single cross-platform entry point (Node.js).
//
// Usage:
//   node scripts/ci.mjs <command> [args...]
//
// Commands:
//   resolve-version          Resolve version from tauri.conf.json, validate tag
//   verify-secrets V1 V2 ... Check env vars are non-empty
//   prepare-changelog VER OUT [RANGE]  Generate release notes markdown
//   update-latest-json ...   Merge platform entry into Tauri updater manifest
//   discord-notify ...       Send Discord webhook notification
//   download-llama PLAT TGT FEAT  Download + validate prebuilt llama libs
//   import-apple-cert        Import .p12 into temporary keychain (macOS)
//   validate-notarization    Check Apple notarization credentials (macOS)
//   free-disk-space          Remove unused toolchains on Linux runners
//   install-protoc-windows   Install protoc via choco or direct download (Windows)
//   self-test                Validate all commands + workflow references
//   dry-run-release          Local release dry-run

import { execSync, spawnSync } from "child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync, rmSync, statSync } from "fs";
import { basename, join, dirname } from "path";
import { tmpdir } from "os";
import { createWriteStream } from "fs";
import https from "https";
import http from "http";

// ── Globals ──────────────────────────────────────────────────────────────────

let CMD = "ci";
const T0 = performance.now();

// ── Helpers ──────────────────────────────────────────────────────────────────

function log(msg) {
  console.log(`[${CMD}] ${msg}`);
}

function ghOutput(key, value) {
  const p = process.env.GITHUB_OUTPUT;
  if (p) writeFileSync(p, `${key}=${value}\n`, { flag: "a" });
  log(`output ${key}=${value}`);
}

function ghEnv(key, value) {
  const p = process.env.GITHUB_ENV;
  if (p) writeFileSync(p, `${key}=${value}\n`, { flag: "a" });
  log(`env ${key}=${value}`);
}

function ghPath(dir) {
  const p = process.env.GITHUB_PATH;
  if (p) writeFileSync(p, `${dir}\n`, { flag: "a" });
  log(`path +=${dir}`);
}

function error(msg) {
  console.log(`::error::[${CMD}] ${msg}`);
}

function warning(msg) {
  console.log(`::warning::[${CMD}] ${msg}`);
}

function confVersion() {
  const text = readFileSync("src-tauri/tauri.conf.json", "utf8");
  const m = text.match(/"version"\s*:\s*"([^"]+)"/);
  if (!m) throw new Error("Could not find version in src-tauri/tauri.conf.json");
  return m[1];
}

function run(cmd, opts = {}) {
  const isArray = Array.isArray(cmd);
  const label = isArray ? cmd.join(" ") : cmd;
  log(`$ ${label}`);
  const args = isArray ? cmd : [cmd];
  const result = spawnSync(args[0], args.slice(1), {
    stdio: opts.capture ? "pipe" : "inherit",
    shell: !isArray,  // shell only for string commands, not arrays
    encoding: "utf8",
    cwd: opts.cwd,
    env: { ...process.env, ...opts.env },
  });
  if (opts.check && result.status !== 0) {
    const err = new Error(`Command failed (exit ${result.status}): ${label}`);
    err.status = result.status;
    err.stdout = result.stdout;
    err.stderr = result.stderr;
    throw err;
  }
  return result;
}

function downloadFile(url, dest) {
  return new Promise((resolve, reject) => {
    const follow = (u) => {
      const proto = u.startsWith("https") ? https : http;
      proto.get(u, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          follow(res.headers.location);
          return;
        }
        if (res.statusCode !== 200) {
          reject(new Error(`HTTP ${res.statusCode} for ${u}`));
          return;
        }
        const ws = createWriteStream(dest);
        res.pipe(ws);
        ws.on("finish", () => ws.close(resolve));
        ws.on("error", reject);
      }).on("error", reject);
    };
    follow(url);
  });
}

function parseArgs(argv, spec) {
  // Minimal arg parser: spec = { "--flag": "key", "--opt": "key" }
  const result = { _: [] };
  let i = 0;
  while (i < argv.length) {
    const arg = argv[i];
    if (spec[arg] !== undefined) {
      if (spec[arg] === true) {
        result[arg.replace(/^--/, "")] = true;
        i++;
      } else {
        result[spec[arg]] = argv[i + 1];
        i += 2;
      }
    } else if (arg.startsWith("--")) {
      // Boolean flag
      result[arg.replace(/^--/, "")] = true;
      i++;
    } else {
      result._.push(arg);
      i++;
    }
  }
  return result;
}

// ── Commands ─────────────────────────────────────────────────────────────────

function cmdResolveVersion() {
  const version = confVersion();
  const event = process.env.GITHUB_EVENT_NAME || "";
  const ref = process.env.GITHUB_REF || "";
  const refName = process.env.GITHUB_REF_NAME || "";
  const dryRun = process.env.DRY_RUN || "false";

  let isRelease = "false";
  let tag = "";

  if (dryRun === "true") {
    tag = `v${version}`;
    log(`[dry-run] Using version from tauri.conf.json: ${version}`);
  } else if (event === "push" && ref.startsWith("refs/tags/v")) {
    isRelease = "true";
    tag = refName;
    const tagVer = tag.replace(/^v/, "");
    if (tagVer !== version) {
      error(`Tag version (${tagVer}) does not match tauri.conf.json version (${version}).`);
      error("Bump the version in src-tauri/tauri.conf.json and src-tauri/Cargo.toml, then re-tag.");
      process.exit(1);
    }
  }

  for (const [k, v] of [["is_release", isRelease], ["version", version], ["tag", tag], ["dry_run", dryRun]]) {
    ghOutput(k, v);
  }
  ghEnv("VERSION", version);
  ghEnv("TAG", tag);
  console.log(`✓ Version: ${version} (release=${isRelease}, dry_run=${dryRun})`);
}

function cmdVerifySecrets(args) {
  const names = args._;
  let ok = true;
  for (const name of names) {
    if (!process.env[name]) {
      error(`Secret '${name}' is empty or not set.`);
      ok = false;
    }
  }
  if (!ok) process.exit(1);
  console.log(`✓ All required secrets are present (${names.length} checked).`);
}

function cmdPrepareChangelog(args) {
  const [version, output] = args._;
  const commitRange = args._[2] || "HEAD~50..HEAD";

  if (!version || !output) {
    console.error("Usage: prepare-changelog <version> <output> [range]");
    process.exit(1);
  }

  // Extract changelog section
  let section = "";
  try {
    const changelog = readFileSync("CHANGELOG.md", "utf8");
    const re = new RegExp(`^## \\[${version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\]`, "m");
    const start = changelog.search(re);
    if (start >= 0) {
      const afterHeader = changelog.indexOf("\n", start) + 1;
      const nextSection = changelog.indexOf("\n## [", afterHeader);
      section = changelog.slice(afterHeader, nextSection > 0 ? nextSection : undefined).trim();
    }
  } catch {}

  // Contributors
  let contributors = "";
  try {
    const r = run(["git", "log", `--format=%aN`, commitRange], { capture: true });
    const seen = new Set();
    for (const name of (r.stdout || "").split("\n").map((l) => l.trim()).filter(Boolean)) {
      if (!seen.has(name)) {
        seen.add(name);
        contributors += `- ${name}\n`;
      }
    }
  } catch {}

  let body = "## Changelog\n\n";
  body += section || `_No changelog section found for version ${version} in CHANGELOG.md._`;
  body += "\n\n## Contributors\n\n";
  body += contributors || `_No commit contributors found in range ${commitRange}._\n`;

  writeFileSync(output, body);
  const lines = body.split("\n").length;
  console.log(`✓ Release notes written to ${output} (${lines} lines)`);
}

function cmdUpdateLatestJson(args) {
  const { platform, url, "sig-file": sigFile, tag, version, upload } = args;
  const signature = readFileSync(sigFile, "utf8").trim();

  // Try to download existing manifest
  const dl = run(["gh", "release", "download", tag, "--pattern", "latest.json", "--output", "latest.json", "--clobber"], { capture: true });

  let manifest;
  if (dl.status === 0 && existsSync("latest.json")) {
    manifest = JSON.parse(readFileSync("latest.json", "utf8"));
  } else {
    let notes = "";
    try {
      const r = run(["git", "tag", "-l", "--format=%(contents)", tag], { capture: true });
      notes = (r.stdout || "").trim();
    } catch {}
    if (!notes) notes = `NeuroSkill\u2122 v${version}`;
    manifest = {
      version,
      notes,
      pub_date: new Date().toISOString().replace(/\.\d+Z$/, "Z"),
      platforms: {},
    };
  }

  manifest.platforms = manifest.platforms || {};
  manifest.platforms[platform] = { url, signature };

  writeFileSync("latest.json", JSON.stringify(manifest, null, 2) + "\n");
  const plats = Object.keys(manifest.platforms).sort().join(", ");
  console.log(`Updated latest.json (${Object.keys(manifest.platforms).length} platform(s): ${plats})`);

  if (upload) {
    run(["gh", "release", "upload", tag, "latest.json", "--clobber"], { check: true });
    console.log(`✓ latest.json uploaded to release ${tag}`);
  }
}

function cmdDiscordNotify(args) {
  const webhook = process.env.DISCORD_WEBHOOK_URL;
  if (!webhook) {
    console.log("⚠ DISCORD_WEBHOOK_URL not set, skipping.");
    return;
  }

  let commit = "";
  try {
    const r = run(["git", "log", "-1", "--format=%s"], { capture: true });
    commit = (r.stdout || "").trim().slice(0, 200);
  } catch {}

  const { status, title, version, tag, platform } = args;
  const releaseUrl = args["release-url"] || "";
  const runUrl = args["run-url"] || "";
  const color = status === "success" ? 3066993 : 15158332;
  const desc = status === "success"
    ? `Build published and ready to download.\n\n**[Download v${version}](${releaseUrl || runUrl})**`
    : `The build failed. Check the run for details.\n\n**[View failed run](${runUrl})**`;

  const payload = JSON.stringify({
    embeds: [{
      title, description: desc, url: runUrl, color,
      fields: [
        { name: "Tag", value: `\`${tag}\``, inline: true },
        { name: "Version", value: `\`${version}\``, inline: true },
        { name: "Platform", value: platform, inline: true },
        { name: "Actor", value: process.env.GITHUB_ACTOR || "ci", inline: true },
        { name: "Commit", value: commit, inline: false },
      ],
      footer: { text: process.env.GITHUB_REPOSITORY || "" },
    }],
  });

  try {
    execSync(`curl -sf -X POST "${webhook}" -H "Content-Type: application/json" -d ${JSON.stringify(payload)}`, { stdio: "pipe" });
  } catch {
    console.log("⚠ Discord notification failed (non-fatal).");
  }
}

async function cmdDownloadLlama(args) {
  const [plat, target, feature] = args._;
  const url = `https://github.com/eugenehp/llama-cpp-rs/releases/latest/download/llama-prebuilt-${plat}-${target}-${feature}-static.tar.gz`;
  const tmp = process.env.RUNNER_TEMP || tmpdir();
  const archive = join(tmp, `llama-prebuilt-${plat}.tar.gz`);
  const dest = join(tmp, `llama-prebuilt-${plat}`);
  mkdirSync(dest, { recursive: true });

  console.log(`Downloading prebuilt llama: ${url}`);
  try {
    await downloadFile(url, archive);
  } catch (e) {
    console.log(`[warn] prebuilt llama artifact unavailable (${e.message}); fallback to source build`);
    return;
  }

  run(["tar", "-xzf", archive, "-C", dest], { check: true });

  // Find root
  let root = dest;
  if (!["lib", "lib64", "bin"].some((d) => existsSync(join(root, d)))) {
    const sub = readdirSync(dest).filter((f) => statSync(join(dest, f)).isDirectory());
    root = sub.length ? join(dest, sub[0]) : "";
  }
  if (!root || !existsSync(root)) {
    console.log("[warn] prebuilt llama archive layout invalid; fallback to source build");
    return;
  }

  // Check for libs
  const exts = { macos: [".a", ".dylib"], linux: [".a", ".so"], windows: [".lib", ".dll"] }[plat] || [".a", ".so", ".lib"];
  const hasLibs = findFiles(root).some((f) => exts.some((e) => f.endsWith(e)));
  if (!hasLibs) {
    console.log("[warn] prebuilt llama archive contains no libs; fallback to source build");
    return;
  }

  // Validate metadata
  const metaPath = join(root, "metadata.json");
  if (existsSync(metaPath)) {
    const meta = JSON.parse(readFileSync(metaPath, "utf8"));
    if (meta.target !== target || !(meta.features || "").includes(feature)) {
      console.log(`[warn] prebuilt metadata mismatch (target=${meta.target} features=${meta.features}); fallback to source build`);
      return;
    }
  }

  ghEnv("LLAMA_PREBUILT_DIR", root);
  ghEnv("LLAMA_PREBUILT_SHARED", "0");
  console.log(`[ok] LLAMA_PREBUILT_DIR=${root}`);
}

function findFiles(dir) {
  const results = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) results.push(...findFiles(full));
    else results.push(full);
  }
  return results;
}

function cmdImportAppleCert() {
  const tmp = process.env.RUNNER_TEMP;
  const keychain = join(tmp, "app-signing.keychain-db");
  const password = execSync("openssl rand -base64 32", { encoding: "utf8" }).trim();

  ghEnv("KEYCHAIN_PATH", keychain);
  ghEnv("KEYCHAIN_PASSWORD", password);

  run(["security", "create-keychain", "-p", password, keychain], { check: true });
  run(["security", "set-keychain-settings", "-lut", "21600", keychain], { check: true });
  run(["security", "unlock-keychain", "-p", password, keychain], { check: true });

  const certPath = join(tmp, "cert.p12");
  writeFileSync(certPath, Buffer.from(process.env.APPLE_CERTIFICATE, "base64"));
  run(["security", "import", certPath, "-k", keychain, "-P", process.env.APPLE_CERTIFICATE_PASSWORD, "-T", "/usr/bin/codesign", "-T", "/usr/bin/security"], { check: true });
  rmSync(certPath);

  run(["security", "set-key-partition-list", "-S", "apple-tool:,apple:", "-s", "-k", password, keychain], { check: true });
  run(["security", "list-keychains", "-d", "user", "-s", keychain, "login.keychain"], { check: true });

  console.log(`✓ Apple Developer certificate imported into ${keychain}`);
}

function cmdValidateNotarization() {
  console.log("Checking notarization credentials …");
  const r = run(["xcrun", "notarytool", "history", "--apple-id", process.env.APPLE_ID, "--password", process.env.APPLE_PASSWORD, "--team-id", process.env.APPLE_TEAM_ID, "--output-format", "json"], { capture: true });
  const output = (r.stdout || "") + (r.stderr || "");
  if (output.includes('"history"')) {
    console.log("✓ Notarization credentials are valid.");
  } else if (/unauthorized|invalid.*credentials|401/i.test(output)) {
    error("Apple notarization credentials are invalid.");
    error("Generate a new app-specific password at https://appleid.apple.com");
    error("Then update APPLE_PASSWORD in: GitHub → Settings → Environments → Release → Secrets");
    process.exit(1);
  } else {
    warning("Could not verify notarization credentials (Apple API may be intermittent).");
    warning(`Output: ${output.slice(0, 500)}`);
    console.log("Proceeding — actual notarization will fail later if credentials are invalid.");
  }
}

function cmdFreeDiskSpace() {
  const dirs = ["/usr/local/lib/android", "/usr/share/dotnet", "/opt/ghc", "/usr/local/.ghcup", "/usr/local/share/powershell", "/usr/local/share/chromium", "/usr/share/swift", "/opt/hostedtoolcache/CodeQL"];
  for (const d of dirs) {
    if (existsSync(d)) run(["sudo", "rm", "-rf", d]);
  }
  run(["sudo", "docker", "image", "prune", "-af"], { capture: true });
  run(["df", "-h", "/"]);
}

function cmdInstallProtocWindows() {
  // Check if already installed
  const which = run(["where", "protoc"], { capture: true });
  if (which.status === 0) {
    run(["protoc", "--version"]);
    return;
  }

  // Try Chocolatey
  let installed = false;
  for (let i = 1; i <= 3; i++) {
    run(["choco", "install", "protoc", "--no-progress", "-y"], { capture: true });
    const check = run(["where", "protoc"], { capture: true });
    if (check.status === 0) {
      installed = true;
      break;
    }
    log(`choco attempt ${i} failed, retrying in ${5 * i}s...`);
    execSync(`timeout /t ${5 * i} /nobreak >nul 2>&1`, { shell: true, stdio: "ignore" });
  }

  if (!installed) {
    console.log("[warn] Chocolatey unavailable; falling back to direct download");
    const ver = "25.3";
    const url = `https://github.com/protocolbuffers/protobuf/releases/download/v${ver}/protoc-${ver}-win64.zip`;
    const tmp = process.env.RUNNER_TEMP || tmpdir();
    const zipPath = join(tmp, `protoc-${ver}-win64.zip`);
    const dest = join(tmp, `protoc-${ver}`);

    execSync(`curl -fSL "${url}" -o "${zipPath}"`, { stdio: "inherit" });
    execSync(`tar -xf "${zipPath}" -C "${dest}"`, { stdio: "inherit" });

    const binDir = join(dest, "bin");
    if (!existsSync(join(binDir, "protoc.exe"))) {
      throw new Error("protoc fallback install failed: protoc.exe not found");
    }
    ghPath(binDir);
    process.env.PATH = `${binDir};${process.env.PATH}`;
    console.log("[ok] Installed protoc via direct download");
  }

  run(["protoc", "--version"]);
}

function cmdSelfTest() {
  const errors = [];
  const commandMap = {
    "resolve-version": cmdResolveVersion, "verify-secrets": cmdVerifySecrets,
    "prepare-changelog": cmdPrepareChangelog, "update-latest-json": cmdUpdateLatestJson,
    "discord-notify": cmdDiscordNotify, "download-llama": cmdDownloadLlama,
    "import-apple-cert": cmdImportAppleCert, "validate-notarization": cmdValidateNotarization,
    "free-disk-space": cmdFreeDiskSpace, "install-protoc-windows": cmdInstallProtocWindows,
    "self-test": cmdSelfTest, "dry-run-release": cmdDryRunRelease,
  };

  for (const [name, fn] of Object.entries(commandMap)) {
    if (typeof fn !== "function") errors.push(`  ${name}: not a function`);
    else log(`✓ ${name}`);
  }

  // Check workflow references
  try {
    const known = new Set(Object.keys(commandMap));
    for (const yml of readdirSync(".github/workflows").filter((f) => f.endsWith(".yml"))) {
      const content = readFileSync(join(".github/workflows", yml), "utf8");
      const re = /scripts\/ci\.mjs\s+([a-z][-a-z]*)/g;
      let m;
      while ((m = re.exec(content))) {
        if (!known.has(m[1])) errors.push(`  ${yml}: unknown command '${m[1]}'`);
      }
    }
  } catch {}

  // Verify confVersion
  try {
    const v = confVersion();
    log(`✓ confVersion() = ${v}`);
  } catch (e) {
    errors.push(`  confVersion(): ${e.message}`);
  }

  if (errors.length) {
    error("self-test failed:");
    errors.forEach((e) => console.log(e));
    process.exit(1);
  }
  log(`✓ all ${Object.keys(commandMap).length} commands OK`);
}

function cmdDryRunRelease(args) {
  const target = args.target || "aarch64-apple-darwin";
  const skipCompile = args["skip-compile"] || false;

  log("Step 1/6: resolve version");
  const version = confVersion();
  log(`version = ${version}`);

  log("Step 2/6: build frontend");
  if (!skipCompile) run(["npm", "run", "build"], { check: true });
  else log("(skipped)");

  log("Step 3/6: cargo build");
  if (!skipCompile) {
    run(["cargo", "build", "--release", "--target", target, "-p", "skill", "--features", "custom-protocol"], { check: true, cwd: "src-tauri" });
    run(["cargo", "build", "--release", "--target", target, "-p", "skill-daemon", "--features", "llm"], { check: true, cwd: "src-tauri" });
  } else log("(skipped)");

  log("Step 4/6: assemble .app bundle");
  const binary = `src-tauri/target/${target}/release/skill`;
  if (!existsSync(binary)) {
    if (skipCompile) warning(`Binary not found at ${binary} — run without --skip-compile first`);
    else { error(`Binary not found at ${binary} after build`); process.exit(1); }
  } else {
    run(["bash", "scripts/assemble-macos-app.sh", target], { check: true });
  }

  log("Step 5/6: prepare changelog");
  const tag = `v${version}`;
  const prev = run(["git", "describe", "--tags", "--abbrev=0", `${tag}^`], { capture: true });
  const prevTag = prev.status === 0 ? prev.stdout.trim() : "";
  const range = prevTag ? `${prevTag}..HEAD` : "HEAD~20..HEAD";
  cmdPrepareChangelog({ _: [version, "dry-run-release-notes.md", range] });

  log("Step 6/6: summary");
  const app = `src-tauri/target/${target}/release/bundle/macos/NeuroSkill.app`;
  console.log("\n" + "=".repeat(60));
  console.log(`  Dry-run release: v${version} (${target})`);
  console.log("=".repeat(60));
  if (existsSync(app)) {
    const size = findFiles(app).reduce((s, f) => s + statSync(f).size, 0);
    console.log(`  .app bundle:  ${app}  (${(size / 1048576).toFixed(1)} MB)`);
  } else console.log("  .app bundle:  (not found)");
  console.log("  changelog:    dry-run-release-notes.md");
  console.log(`\n  To run:   open '${app}'`);
  console.log(`  To sign:  APPLE_SIGNING_IDENTITY=... bash scripts/assemble-macos-app.sh ${target}`);
  console.log("=".repeat(60) + "\n");
}

// ── CLI ──────────────────────────────────────────────────────────────────────

const COMMANDS = {
  "resolve-version": cmdResolveVersion,
  "verify-secrets": cmdVerifySecrets,
  "prepare-changelog": cmdPrepareChangelog,
  "update-latest-json": (a) => cmdUpdateLatestJson(a),
  "discord-notify": (a) => cmdDiscordNotify(a),
  "download-llama": (a) => cmdDownloadLlama(a),
  "import-apple-cert": cmdImportAppleCert,
  "validate-notarization": cmdValidateNotarization,
  "free-disk-space": cmdFreeDiskSpace,
  "install-protoc-windows": cmdInstallProtocWindows,
  "self-test": cmdSelfTest,
  "dry-run-release": (a) => cmdDryRunRelease(a),
};

async function main() {
  const argv = process.argv.slice(2);
  const command = argv[0];

  if (!command || command === "--help" || command === "-h") {
    console.log("Usage: node scripts/ci.mjs <command> [args...]");
    console.log("\nCommands:");
    for (const name of Object.keys(COMMANDS)) console.log(`  ${name}`);
    process.exit(0);
  }

  if (!COMMANDS[command]) {
    error(`Unknown command: ${command}`);
    process.exit(1);
  }

  CMD = command;
  const args = parseArgs(argv.slice(1), {
    "--platform": "platform", "--url": "url", "--sig-file": "sig-file",
    "--tag": "tag", "--version": "version", "--upload": true,
    "--status": "status", "--title": "title", "--release-url": "release-url",
    "--run-url": "run-url", "--target": "target", "--skip-compile": true,
  });

  log("starting");
  try {
    await COMMANDS[command](args);
  } catch (e) {
    if (e.status !== undefined) {
      error(`Command failed (exit ${e.status}): ${e.message}`);
      if (e.stdout) console.log(`[${CMD}] stdout:\n${e.stdout.trim()}`);
      if (e.stderr) console.log(`[${CMD}] stderr:\n${e.stderr.trim()}`);
    } else {
      error(`${e.constructor.name}: ${e.message}`);
      console.error(e.stack);
    }
    const elapsed = ((performance.now() - T0) / 1000).toFixed(1);
    error(`Failed after ${elapsed}s`);
    process.exit(1);
  }
  const elapsed = ((performance.now() - T0) / 1000).toFixed(1);
  log(`done (${elapsed}s)`);
}

main();
