// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

// Rollup calls plugin `onLog` hooks (not `onwarn`) — `onLog` is in
// Vite's ROLLUP_HOOKS allowlist so it survives injectEnvironmentAndFilterToHooks
// and is wired into Rollup's getLogger() for every build pass (SSR + client).
// Returning `false` from a plugin onLog handler suppresses the entry.
// `onwarn` plugin hooks are NOT in ROLLUP_HOOKS and are never called.
/** @type {import('vite').Plugin} */
const suppressUnusedImportWarnings = {
  name: "suppress-node-modules-unused-import-warnings",
  apply: "build",
  onLog(level, log) {
    // "UNUSED_EXTERNAL_IMPORT" is purely informational for third-party
    // packages we cannot modify; drop the noise unconditionally when
    // every reporting file lives inside node_modules.
    if (
      level === "warn" &&
      log.code === "UNUSED_EXTERNAL_IMPORT" &&
      log.ids?.length &&
      log.ids.every((id) => id.includes("node_modules"))
    ) return false;
  },
};

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [
    tailwindcss(),
    sveltekit(),
    suppressUnusedImportWarnings,
  ],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
