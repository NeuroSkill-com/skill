// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Reactive window-title helper.
 *
 * Dynamically imports Tauri's getCurrentWindow inside a reactive $effect so
 * the SSR bundle never receives a static import Rollup would flag as unused.
 * The effect re-runs whenever the active locale changes, keeping the OS title
 * bar in sync with the selected language.
 *
 * Usage — call once at the top level of a component's <script> block:
 *
 *   import { useWindowTitle } from "$lib/stores/window-title.svelte";
 *   useWindowTitle("window.title.settings");
 */

import { t } from "$lib/i18n/index.svelte";

let lastAppliedTitle = "";

/**
 * Registers a reactive `$effect` that immediately sets the OS window title
 * to `t(key)` and keeps it updated whenever the locale changes.
 *
 * Must be called synchronously during component initialisation
 * (top level of a `<script>` block, or a function called from there).
 *
 * The Tauri API is imported dynamically so that the SSR bundle does not
 * receive a static import that Rollup would flag as unused (effects are
 * no-ops during SSR, so a static import would never be referenced).
 */
export function useWindowTitle(key: string): void {
  $effect(() => {
    const title = t(key);
    if (title === lastAppliedTitle) return;
    lastAppliedTitle = title;
    import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
      getCurrentWindow().setTitle(title).catch(e => console.warn("[window] setTitle failed:", e));
    });
  });
}
