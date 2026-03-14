// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

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

// ── Tailwind v4 + Svelte compatibility shim ──────────────────────────────────
//
// @tailwindcss/vite v4.2 registers its serve & build transform plugins with
// `enforce: "pre"`.  Both match `*.svelte?svelte&type=style&lang.css` via the
// `&lang.css` suffix in their ID filter.  In the dev server's pre-transform
// phase the Svelte compiler has not yet extracted the `<style>` block, so the
// raw `.svelte` file (script + markup + style) is handed to Tailwind's CSS
// parser, which crashes on `import { onMount }` etc.
//
// Fix: patch every Tailwind plugin's transform handler (both serve and build)
// to skip `.svelte` style virtual modules entirely.  Svelte component `<style>`
// blocks contain plain CSS — they don't use Tailwind `@apply` or utilities, so
// Tailwind processing is not needed.  Tailwind utilities used in Svelte
// templates are resolved through the main `app.css` import.

function patchTailwindForSvelte() {
  const plugins = tailwindcss();
  const pluginArray = Array.isArray(plugins) ? plugins : [plugins];

  for (const p of pluginArray) {
    if (!p.transform) continue;

    const origTransform = p.transform;

    // The transform can be a plain function or an object { filter, handler }.
    if (typeof origTransform === "function") {
      p.transform = function (code, id) {
        if (id.includes(".svelte?") && id.includes("&lang.css")) return;
        return origTransform.call(this, code, id);
      };
    } else if (typeof origTransform === "object" && origTransform.handler) {
      const origHandler = origTransform.handler;
      origTransform.handler = function (code, id) {
        if (id.includes(".svelte?") && id.includes("&lang.css")) return;
        return origHandler.call(this, code, id);
      };
    }
  }

  return pluginArray;
}

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [
    sveltekit(),
    ...patchTailwindForSvelte(),
    suppressUnusedImportWarnings,
  ],

  test: {
    exclude: [
      "**/node_modules/**",
      "**/dist/**",
      "**/build/**",
      "**/.{idea,git,cache,output,temp}/**",
      "**/{karma,rollup,webpack,vite,vitest,jest,ava,babel,nyc,cypress,tsup,build}.config.*",
      "src-tauri/target/**",
    ],
  },

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
    fs: {
      allow: [path.resolve("."), path.resolve("./src")],
    },
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
