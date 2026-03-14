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

  // ── Changelog parsing ─────────────────────────────────────────────────────
  // Splits the raw CHANGELOG into one entry per ## [...] section and retains
  // all sections that have a non-empty body (including [Unreleased]).

  interface VersionMeta {
    version: string; // e.g. "0.0.24" or "Unreleased"
    date:    string; // e.g. "2026-03-12", empty for Unreleased
    body:    string; // trimmed markdown body of that section
  }

  function extractAllVersions(raw: string): VersionMeta[] {
    // Matches: ## [version]  or  ## [version] — date
    const headerRe = /^## \[([^\]]+)\](?:[^\S\n]*[—–\-]+[^\S\n]*(\S+))?[^\n]*/gm;
    const headers: Array<{ hdrStart: number; bodyStart: number; version: string; date: string }> = [];
    let m: RegExpExecArray | null;
    while ((m = headerRe.exec(raw)) !== null) {
      headers.push({
        hdrStart:  m.index,
        bodyStart: m.index + m[0].length,
        version:   m[1].trim(),
        date:      m[2]?.trim() ?? "",
      });
    }
    return headers
      .map(({ hdrStart, bodyStart, version, date }, i) => {
        const bodyEnd = i + 1 < headers.length ? headers[i + 1].hdrStart : raw.length;
        return { version, date, body: raw.slice(bodyStart, bodyEnd).trim() };
      })
      .filter(v => v.body.length > 0);
  }

  const allVersions = extractAllVersions(changelogRaw);

  // Default to the first released version (skip [Unreleased] as the landing
  // view so users see actual shipped changes when the window opens).
  const firstRelIdx = allVersions.findIndex(v => v.version.toLowerCase() !== "unreleased");
  const defaultIdx  = firstRelIdx >= 0 ? firstRelIdx : 0;

  // ── State ──────────────────────────────────────────────────────────────────
  let currentIdx  = $state(defaultIdx);
  let scrollEl    = $state<HTMLDivElement | undefined>();
  // Seed appVersion from the changelog so dismiss_whats_new never persists "…"
  // in the rare race where the user clicks "Got it" before the IPC resolves.
  let appVersion  = $state(allVersions[defaultIdx]?.version ?? "…");

  const current = $derived(allVersions[currentIdx]);

  // Reset scroll to top whenever the user navigates to a different section.
  $effect(() => {
    currentIdx;
    scrollEl?.scrollTo({ top: 0, behavior: "instant" });
  });

  // ── Lifecycle ──────────────────────────────────────────────────────────────
  onMount(async () => {
    try {
      appVersion = await invoke<string>("get_app_version");
    } catch {
      appVersion = allVersions[defaultIdx]?.version ?? "?";
    }
  });

  // ── Navigation ─────────────────────────────────────────────────────────────
  function goNewer() { if (currentIdx > 0) currentIdx--; }
  function goOlder() { if (currentIdx < allVersions.length - 1) currentIdx++; }

  // ── Dismiss — handled entirely in Rust (saves version + closes window) ────
  async function dismiss() {
    await invoke("dismiss_whats_new", { version: appVersion });
  }
</script>

<main class="h-full min-h-0 bg-background text-foreground flex flex-col overflow-hidden select-none">

  {#if current}
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
            {current.version === "Unreleased"
              ? t("whatsNew.unreleased")
              : t("whatsNew.version", { version: current.version })}
            {#if current.date}&nbsp;·&nbsp;{current.date}{/if}
          </span>
        </div>
      </div>
    </div>

    <!-- ── Version navigation bar ───────────────────────────────────────────── -->
    {#if allVersions.length > 1}
      <div class="shrink-0 flex items-center justify-between gap-2 px-3 py-1.5
                  border-b border-border dark:border-white/[0.06]
                  bg-muted/30 dark:bg-white/[0.02]">

        <!-- ← Newer -->
        <button
          onclick={goNewer}
          disabled={currentIdx === 0}
          aria-label={t("whatsNew.newer")}
          class="inline-flex items-center gap-1 px-2.5 py-1 rounded text-[0.7rem]
                 font-medium text-muted-foreground
                 hover:text-foreground hover:bg-muted/60
                 disabled:opacity-30 disabled:cursor-not-allowed
                 transition-colors cursor-pointer select-none">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8"
               class="w-3 h-3 shrink-0" aria-hidden="true">
            <path d="M10 3L5 8l5 5" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
          {t("whatsNew.newer")}
        </button>

        <!-- Version picker dropdown -->
        <div class="relative flex-1 min-w-0 max-w-[13rem]">
          <select
            bind:value={currentIdx}
            class="w-full appearance-none text-center text-[0.7rem] font-medium
                   text-foreground bg-background dark:bg-[#14141e]
                   border border-border/60 dark:border-white/[0.10]
                   rounded pl-2 pr-6 py-0.5 cursor-pointer
                   focus:outline-none focus:ring-1 focus:ring-violet-500/40">
            {#each allVersions as v, i (i)}
              <option value={i} class="bg-background text-foreground">
                {v.version === "Unreleased"
                  ? "Unreleased"
                  : `v${v.version}${v.date ? "  ·  " + v.date : ""}`}
              </option>
            {/each}
          </select>
          <span class="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground">
            <svg viewBox="0 0 16 16" fill="currentColor" class="w-3 h-3" aria-hidden="true">
              <path d="M3 6l5 5 5-5H3z"/>
            </svg>
          </span>
        </div>

        <!-- Older → -->
        <button
          onclick={goOlder}
          disabled={currentIdx === allVersions.length - 1}
          aria-label={t("whatsNew.older")}
          class="inline-flex items-center gap-1 px-2.5 py-1 rounded text-[0.7rem]
                 font-medium text-muted-foreground
                 hover:text-foreground hover:bg-muted/60
                 disabled:opacity-30 disabled:cursor-not-allowed
                 transition-colors cursor-pointer select-none">
          {t("whatsNew.older")}
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8"
               class="w-3 h-3 shrink-0" aria-hidden="true">
            <path d="M6 3l5 5-5 5" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        </button>

      </div>
    {/if}

    <!-- ── Scrollable changelog body ──────────────────────────────────────── -->
    <div bind:this={scrollEl}
         class="wn-body min-h-0 flex-1 overflow-y-auto overscroll-contain px-6 py-5 text-[0.78rem]">
      <MarkdownRenderer content={current.body} />
    </div>

    <!-- ── Footer ─────────────────────────────────────────────────────────── -->
    <div class="px-6 py-4 border-t border-border dark:border-white/[0.06]
                flex items-center justify-between shrink-0">
      <span class="text-[0.68rem] text-muted-foreground/40 tabular-nums select-none">
        {currentIdx + 1}&thinsp;/&thinsp;{allVersions.length}
      </span>
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

<!-- no component-scoped styles — all styling via Tailwind utilities -->
