<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { onMount }        from "svelte";
  import { invoke }         from "@tauri-apps/api/core";
  import { t }              from "$lib/i18n/index.svelte";
  import { useWindowTitle } from "$lib/window-title.svelte";
  import MarkdownRenderer   from "$lib/MarkdownRenderer.svelte";
  import changelogRaw       from "../../../CHANGELOG.md?raw";

  useWindowTitle("whatsNew.title");

  // ── Changelog extraction ───────────────────────────────────────────────────
  // Must run before $state initialisation below — `const` is not hoisted so
  // referencing `latest` before its declaration is a ReferenceError (TDZ).

  interface VersionMeta {
    version: string;
    date:    string;
    body:    string;
  }

  function extractLatest(raw: string): VersionMeta | undefined {
    const headerRe = /^##\s+\[([^\]]+)\][^\S\n]*[—–-]+[^\S\n]*(\S+)/m;
    const match = raw.match(headerRe);
    if (!match) return undefined;

    const version     = match[1].trim();
    const date        = match[2].trim();
    const afterHeader = raw.slice(raw.indexOf(match[0]) + match[0].length);
    const nextBlock   = afterHeader.search(/^##\s+\[/m);
    const body        = (nextBlock === -1
      ? afterHeader
      : afterHeader.slice(0, nextBlock)
    ).trim();

    return { version, date, body };
  }

  const latest = extractLatest(changelogRaw);

  // ── State ──────────────────────────────────────────────────────────────────
  // Seed from the CHANGELOG so dismiss_whats_new never persists "…" in the
  // rare race where the user clicks "Got it" before get_app_version() resolves.
  let appVersion = $state(latest?.version ?? "…");

  // ── Lifecycle ──────────────────────────────────────────────────────────────
  onMount(async () => {
    try {
      appVersion = await invoke<string>("get_app_version");
    } catch {
      appVersion = latest?.version ?? "?";
    }
  });

  // ── Dismiss — handled entirely in Rust (saves version + closes window) ────
  async function dismiss() {
    await invoke("dismiss_whats_new", { version: appVersion });
  }
</script>

<main class="h-full min-h-0 bg-background text-foreground flex flex-col overflow-hidden select-none">

  {#if latest}
    <!-- ── Gradient header ──────────────────────────────────────────────────── -->
    <div class="px-6 pt-5 pb-4 shrink-0
                bg-gradient-to-br from-violet-500/10 via-blue-500/8 to-sky-500/10
                dark:from-violet-500/15 dark:via-blue-500/12 dark:to-sky-500/15
                border-b border-border dark:border-white/[0.06]">
      <div class="flex items-center gap-3">
        <!-- Icon badge -->
        <div class="flex items-center justify-center w-10 h-10 rounded-xl shrink-0
                    bg-gradient-to-br from-violet-500 to-blue-600
                    shadow-lg shadow-violet-500/30 dark:shadow-violet-500/40">
          <svg viewBox="0 0 24 24" fill="none" stroke="white"
               stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"
               class="w-5 h-5" aria-hidden="true">
            <path d="M12 3l1.5 5.5L19 10l-5.5 1.5L12 17l-1.5-5.5L5 10l5.5-1.5z"/>
            <path d="M5 3l.75 2.25L8 6l-2.25.75L5 9l-.75-2.25L2 6l2.25-.75z" stroke-width="1.5"/>
          </svg>
        </div>
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.9rem] font-bold leading-tight text-foreground">
            {t("whatsNew.title")}
          </span>
          <span class="text-[0.6rem] font-semibold text-muted-foreground/60 tracking-wide uppercase">
            {t("whatsNew.version", { version: latest.version })}
            &nbsp;·&nbsp;
            {latest.date}
          </span>
        </div>
      </div>
    </div>

    <!-- ── Scrollable changelog body ──────────────────────────────────────── -->
    <div class="wn-body min-h-0 flex-1 overflow-y-auto overscroll-contain px-6 py-5 text-[0.78rem]">
      <MarkdownRenderer content={latest.body} />
    </div>

    <!-- ── Footer ─────────────────────────────────────────────────────────── -->
    <div class="px-6 py-4 border-t border-border dark:border-white/[0.06]
                flex items-center justify-end shrink-0">
      <button
        onclick={dismiss}
        class="px-6 h-9 rounded-lg text-[0.78rem] font-semibold text-white
               bg-gradient-to-r from-violet-500 to-blue-600
               hover:from-violet-600 hover:to-blue-700
               shadow shadow-violet-500/20 dark:shadow-violet-500/30
               transition-all cursor-pointer select-none">
        {t("whatsNew.gotIt")}
      </button>
    </div>
  {/if}

</main>

<style>
</style>
