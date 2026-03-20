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
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  // Side-effect: initialises locale from localStorage / navigator.language
  import "$lib/i18n/index.svelte";
  import { initLocaleFromSettings } from "$lib/i18n/index.svelte";
  // Side-effect: initialises theme from localStorage / system preference
  import "$lib/stores/theme.svelte";
  import { initFromSettings as initThemeFromSettings, toggleTheme } from "$lib/stores/theme.svelte";
  // Side-effect: initialises font size from localStorage
  import "$lib/stores/font-size.svelte";
  // Side-effect: initialises chart color scheme from localStorage
  import "$lib/stores/chart-colors.svelte";
  // Side-effect: fetches canonical app name from Rust backend
  import "$lib/stores/app-name.svelte";
  import { ToastContainer } from "$lib/components/ui/toast";
  import { addToast, type ToastLevel } from "$lib/stores/toast.svelte";
  import KeyboardShortcuts from "$lib/KeyboardShortcuts.svelte";
  import CommandPalette    from "$lib/CommandPalette.svelte";
  import WhatsNew          from "$lib/WhatsNew.svelte";
  import CustomTitleBar    from "$lib/CustomTitleBar.svelte";

  let { children }: { children: Snippet } = $props();

  // Listen for toast events emitted from the Rust backend and relay them
  // into the in-app toast store.  Each window gets its own listener so
  // toasts appear in whichever window is currently visible.
  const unlisteners: UnlistenFn[] = [];
  onMount(async () => {
    // Sentinel read by the Rust side when re-showing this window from the
    // system tray or Dock after a long idle period.  Its absence means the
    // WKWebView web-content process was killed by macOS (memory pressure)
    // and the page is blank — Rust detects this and triggers a reload.
    (window as unknown as Record<string, unknown>)["__skill_loaded"] = true;

    // Reveal the main window now that the page has fully rendered.
    // Deferring win.show() to this point eliminates the "white screen on
    // macOS first launch" issue caused by calling show() in Tauri setup
    // before WKWebView has loaded any content.  For secondary windows
    // (settings, help, etc.) the command is a no-op.
    invoke("show_main_window").catch(e => console.warn("[layout] show_main_window failed:", e));

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

    // ── Cmd/Ctrl+W to close (or hide) the current window ─────────────────
    function handleCloseShortcut(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === "w") {
        e.preventDefault();
        getCurrentWindow().close();
      }
    }
    window.addEventListener("keydown", handleCloseShortcut);
    unlisteners.push(() => window.removeEventListener("keydown", handleCloseShortcut));
  });
  onDestroy(() => unlisteners.forEach((u) => u()));
</script>

<a href="#main-content" class="skip-link">Skip to content</a>
<div aria-live="polite" class="sr-only" id="a11y-announcer"></div>
<CustomTitleBar />
<ToastContainer />
<KeyboardShortcuts />
<CommandPalette />
<WhatsNew />
<div id="main-content">
  {@render children()}
</div>
