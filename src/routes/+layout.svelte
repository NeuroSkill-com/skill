<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import "../app.css";
  import { onMount, onDestroy } from "svelte";
  import type { Snippet } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  // Side-effect: initialises locale from localStorage / navigator.language
  import "$lib/i18n/index.svelte";
  import { initLocaleFromSettings } from "$lib/i18n/index.svelte";
  // Side-effect: initialises theme from localStorage / system preference
  import "$lib/theme-store.svelte";
  import { initFromSettings as initThemeFromSettings, toggleTheme } from "$lib/theme-store.svelte";
  // Side-effect: initialises font size from localStorage
  import "$lib/font-size-store.svelte";
  // Side-effect: initialises chart color scheme from localStorage
  import "$lib/chart-colors-store.svelte";
  // Side-effect: fetches canonical app name from Rust backend
  import "$lib/app-name-store.svelte";
  import { ToastContainer } from "$lib/components/ui/toast";
  import { addToast, type ToastLevel } from "$lib/toast-store.svelte";
  import KeyboardShortcuts from "$lib/KeyboardShortcuts.svelte";
  import CommandPalette    from "$lib/CommandPalette.svelte";

  let { children }: { children: Snippet } = $props();

  // Listen for toast events emitted from the Rust backend and relay them
  // into the in-app toast store.  Each window gets its own listener so
  // toasts appear in whichever window is currently visible.
  const unlisteners: UnlistenFn[] = [];
  onMount(async () => {
    // Restore theme & language from settings.json (overrides localStorage)
    await Promise.all([initThemeFromSettings(), initLocaleFromSettings()]);

    unlisteners.push(
      await listen<{ level: ToastLevel; title: string; message: string }>(
        "toast",
        (ev) => addToast(ev.payload.level, ev.payload.title, ev.payload.message),
      ),
    );
    unlisteners.push(
      await listen("toggle-theme", () => toggleTheme()),
    );
  });
  onDestroy(() => unlisteners.forEach((u) => u()));
</script>

<a href="#main-content" class="skip-link">Skip to content</a>
<div aria-live="polite" class="sr-only" id="a11y-announcer"></div>
<ToastContainer />
<KeyboardShortcuts />
<CommandPalette />
<div id="main-content">
  {@render children()}
</div>
