#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * generate-icons.js — Generate all Tauri app icons from a single source image.
 *
 * Takes any SVG, PNG, or JPEG source image and produces every icon file needed
 * by the Tauri build: PNGs at all required sizes, .ico (Windows), .icns (macOS),
 * and tray icons.
 *
 * Usage:
 *   node generate-icons.js <source-image>
 *   node generate-icons.js logo.svg
 *   node generate-icons.js icon.png
 *   node generate-icons.js brand.jpg
 *
 * Requirements:
 *   - Node ≥ 18
 *   - rsvg-convert  (SVG sources)   — brew install librsvg / apk add librsvg / apt install librsvg2-bin
 *   - ImageMagick   (PNG/JPEG sources) — brew install imagemagick / apk add imagemagick / apt install imagemagick
 *     ImageMagick 6 (`convert`) and ImageMagick 7 (`magick`) are both supported.
 *
 * Output directory: src-tauri/icons/
 *   ├── 32x32.png
 *   ├── 128x128.png
 *   ├── 128x128@2x.png      (256px)
 *   ├── icon.png             (512px)
 *   ├── icon.ico             (16/32/48/256px multi-layer)
 *   ├── icon.icns            (16/32/64/128/256/512/1024px)
 *   ├── Square30x30Logo.png
 *   ├── Square44x44Logo.png
 *   ├── Square71x71Logo.png
 *   ├── Square89x89Logo.png
 *   ├── Square107x107Logo.png
 *   ├── Square142x142Logo.png
 *   ├── Square150x150Logo.png
 *   ├── Square284x284Logo.png
 *   ├── Square310x310Logo.png
 *   ├── StoreLogo.png        (50px)
 *   ├── tray-bt-off.png      (22px)
 *   ├── tray-connected.png   (22px)
 *   ├── tray-disconnected.png(22px)
 *   └── tray-scanning.png    (22px)
 */

import { execSync } from "child_process";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// ── Config ────────────────────────────────────────────────────────────────────

const ICONS_DIR  = path.join(__dirname, "src-tauri", "icons");
const STATIC_DIR = path.join(__dirname, "static");

const PNG_TARGETS = [
  { file: "32x32.png",             size: 32  },
  { file: "128x128.png",           size: 128 },
  { file: "128x128@2x.png",        size: 256 },
  { file: "icon.png",              size: 512 },
  { file: "Square30x30Logo.png",   size: 30  },
  { file: "Square44x44Logo.png",   size: 44  },
  { file: "Square71x71Logo.png",   size: 71  },
  { file: "Square89x89Logo.png",   size: 89  },
  { file: "Square107x107Logo.png", size: 107 },
  { file: "Square142x142Logo.png", size: 142 },
  { file: "Square150x150Logo.png", size: 150 },
  { file: "Square284x284Logo.png", size: 284 },
  { file: "Square310x310Logo.png", size: 310 },
  { file: "StoreLogo.png",         size: 50  },
];

// Tray icons are NOT generated here — they are status-specific (different colors
// per Muse connection state: green=connected, gray=disconnected, yellow=scanning,
// red-slash=bt_off). They live in src-tauri/icons/tray-*.png and are hand-crafted
// SVGs rasterized to 32×32. Edit them manually if the design changes.
const TRAY_ICONS = [];
const TRAY_SIZE = 32;

const ICO_SIZES = [16, 32, 48, 256];

const ICNS_TYPES = [
  { code: "icp4", size: 16   },
  { code: "icp5", size: 32   },
  { code: "icp6", size: 64   },
  { code: "ic07", size: 128  },
  { code: "ic08", size: 256  },
  { code: "ic09", size: 512  },
  { code: "ic10", size: 1024 },
];

// ── Helpers ───────────────────────────────────────────────────────────────────

function die(msg) {
  console.error(`\x1b[31mError:\x1b[0m ${msg}`);
  process.exit(1);
}

/** Return 'svg' or 'raster' based on file extension. */
function fileType(srcPath) {
  return path.extname(srcPath).toLowerCase() === ".svg" ? "svg" : "raster";
}

/**
 * Find an available ImageMagick binary.
 * ImageMagick 7 ships as `magick`; IM6 ships as `convert`.
 * Returns the command string (e.g. "magick" or "convert") or null.
 */
function findImageMagick() {
  for (const cmd of ["magick", "convert"]) {
    try {
      execSync(`${cmd} --version`, { stdio: "pipe" });
      return cmd;
    } catch {
      // not found, try next
    }
  }
  return null;
}

/**
 * Resize any image to `size`×`size` and write a PNG to `outPath`.
 * - SVG sources use rsvg-convert.
 * - Raster (PNG/JPEG) sources use ImageMagick, preserving transparency and
 *   fitting the image inside the square canvas (no distortion).
 */
function rasterize(src, outPath, size, { imCmd, type }) {
  if (type === "svg") {
    execSync(`rsvg-convert -w ${size} -h ${size} "${src}" -o "${outPath}"`, {
      stdio: ["pipe", "pipe", "pipe"],
    });
  } else {
    // Fit inside size×size, pad transparent background, output PNG.
    execSync(
      `${imCmd} "${src}" -resize ${size}x${size} -background none` +
        ` -gravity center -extent ${size}x${size} "${outPath}"`,
      { stdio: ["pipe", "pipe", "pipe"] }
    );
  }
}

function buildIco(pngPaths, outPath) {
  const pngs = pngPaths.map((p) => fs.readFileSync(p));
  const count = pngs.length;
  const headerSize = 6;
  const dirEntrySize = 16;
  let offset = headerSize + dirEntrySize * count;

  // ICO header: reserved(2) + type(2, 1=ico) + count(2)
  const header = Buffer.alloc(headerSize);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(count, 4);

  const entries = [];
  for (let i = 0; i < count; i++) {
    const e = Buffer.alloc(dirEntrySize);
    const s = ICO_SIZES[i] >= 256 ? 0 : ICO_SIZES[i];
    e.writeUInt8(s, 0);                       // width  (0 = 256+)
    e.writeUInt8(s, 1);                       // height (0 = 256+)
    e.writeUInt8(0, 2);                       // color palette
    e.writeUInt8(0, 3);                       // reserved
    e.writeUInt16LE(1, 4);                    // color planes
    e.writeUInt16LE(32, 6);                   // bits per pixel
    e.writeUInt32LE(pngs[i].length, 8);       // image data size
    e.writeUInt32LE(offset, 12);              // offset to image data
    offset += pngs[i].length;
    entries.push(e);
  }

  fs.writeFileSync(outPath, Buffer.concat([header, ...entries, ...pngs]));
}

function buildIcns(pngPaths, outPath) {
  const entries = [];
  let totalSize = 8; // magic(4) + filesize(4)

  for (let i = 0; i < ICNS_TYPES.length; i++) {
    const png = fs.readFileSync(pngPaths[i]);
    const entrySize = 8 + png.length; // type(4) + size(4) + data
    const buf = Buffer.alloc(entrySize);
    buf.write(ICNS_TYPES[i].code, 0, 4, "ascii");
    buf.writeUInt32BE(entrySize, 4);
    png.copy(buf, 8);
    entries.push(buf);
    totalSize += entrySize;
  }

  const header = Buffer.alloc(8);
  header.write("icns", 0, 4, "ascii");
  header.writeUInt32BE(totalSize, 4);

  fs.writeFileSync(outPath, Buffer.concat([header, ...entries]));
}

// ── Main ──────────────────────────────────────────────────────────────────────

function main() {
  const src = process.argv[2];
  if (!src) {
    console.log("Usage: node generate-icons.js <source-image>");
    console.log("       Accepts SVG, PNG, or JPEG.");
    process.exit(1);
  }

  const srcPath = path.resolve(src);
  if (!fs.existsSync(srcPath)) die(`Source file not found: ${srcPath}`);

  const type = fileType(srcPath);

  // Validate required external tools
  let imCmd = null;
  if (type === "svg") {
    try {
      execSync("rsvg-convert --version", { stdio: "pipe" });
    } catch {
      die(
        "rsvg-convert not found — required for SVG sources.\n" +
        "  macOS : brew install librsvg\n" +
        "  Alpine: apk add librsvg\n" +
        "  Debian: apt install librsvg2-bin"
      );
    }
  } else {
    imCmd = findImageMagick();
    if (!imCmd) {
      die(
        "ImageMagick not found — required for PNG/JPEG sources.\n" +
        "  macOS : brew install imagemagick\n" +
        "  Alpine: apk add imagemagick\n" +
        "  Debian: apt install imagemagick"
      );
    }
  }

  const ctx = { imCmd, type };

  // Ensure output dir exists
  fs.mkdirSync(ICONS_DIR, { recursive: true });

  const tmpFiles = [];
  const tmp = (name) => {
    const p = path.join(ICONS_DIR, `.tmp_${name}`);
    tmpFiles.push(p);
    return p;
  };

  try {
    // 1. Generate all PNG targets
    console.log(`\x1b[1mSource:\x1b[0m ${srcPath} \x1b[2m(${type})\x1b[0m`);
    console.log(`\x1b[1mOutput:\x1b[0m ${ICONS_DIR}/\n`);

    for (const t of PNG_TARGETS) {
      const out = path.join(ICONS_DIR, t.file);
      rasterize(srcPath, out, t.size, ctx);
      console.log(`  \x1b[32m✓\x1b[0m ${t.file} (${t.size}×${t.size})`);
    }

    // 2. Tray icons
    for (const name of TRAY_ICONS) {
      const out = path.join(ICONS_DIR, name);
      rasterize(srcPath, out, TRAY_SIZE, ctx);
      console.log(`  \x1b[32m✓\x1b[0m ${name} (${TRAY_SIZE}×${TRAY_SIZE})`);
    }

    // 3. ICO (multi-layer)
    const icoPngs = ICO_SIZES.map((s) => {
      const p = tmp(`ico_${s}.png`);
      rasterize(srcPath, p, s, ctx);
      return p;
    });
    buildIco(icoPngs, path.join(ICONS_DIR, "icon.ico"));
    console.log(`  \x1b[32m✓\x1b[0m icon.ico (${ICO_SIZES.join("/")}px layers)`);

    // 4. ICNS (multi-layer)
    const icnsPngs = ICNS_TYPES.map((t) => {
      const p = tmp(`icns_${t.size}.png`);
      rasterize(srcPath, p, t.size, ctx);
      return p;
    });
    buildIcns(icnsPngs, path.join(ICONS_DIR, "icon.icns"));
    console.log(`  \x1b[32m✓\x1b[0m icon.icns (${ICNS_TYPES.map((t) => t.size).join("/")}px layers)`);

    // 5. Sync static/icon.png used by the web frontend (e.g. the About window)
    const tauriIcon  = path.join(ICONS_DIR,  "icon.png");
    const staticIcon = path.join(STATIC_DIR, "icon.png");
    fs.copyFileSync(tauriIcon, staticIcon);
    console.log(`  \x1b[32m✓\x1b[0m static/icon.png (synced from icon.png)`);

    console.log(`\n\x1b[32mDone.\x1b[0m ${PNG_TARGETS.length + TRAY_ICONS.length + 2} files generated.`);
  } finally {
    // Clean up temp files
    for (const f of tmpFiles) {
      try { fs.unlinkSync(f); } catch {}
    }
  }
}

main();
