<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { Button }   from "$lib/components/ui/button";
  import { Textarea } from "$lib/components/ui/textarea";
  import { t }        from "$lib/i18n/index.svelte";
  import { labelTitlebarState } from "$lib/label-titlebar.svelte";
  import { useWindowTitle } from "$lib/window-title.svelte";
  import { fmtElapsed } from "$lib/format";

  // ── State ──────────────────────────────────────────────────────────────────
  let text          = $state("");
  let context       = $state("");
  let saving        = $state(false);
  let error         = $state("");
  let elapsed       = $state(0);
  let labelStartUtc = 0;                         // set once in onMount
  let recentLabels  = $state<string[]>([]);
  let historyIndex  = $state(-1);
  let draftText     = $state("");

  // bind:ref requires $state in Svelte 5
  let textareaEl  = $state<HTMLTextAreaElement | null>(null);
  let contextEl   = $state<HTMLTextAreaElement | null>(null);
  let timer: ReturnType<typeof setInterval> | null = null;

  // ── Helpers ────────────────────────────────────────────────────────────────
  const isMac = typeof navigator !== "undefined" && navigator.platform?.includes("Mac");

  /** Close via Rust command — avoids importing webviewWindow (causes Vite reload). */
  async function closeWindow() {
    await invoke("close_label_window");
  }

  async function submit() {
    if (!text.trim() || saving) return;
    saving = true;
    error  = "";
    try {
      await invoke("submit_label", { labelStartUtc, text: text.trim(), context: context.trim() });
      await closeWindow();
    } catch (e) {
      error  = String(e);
      saving = false;
    }
  }

  async function loadRecentLabels() {
    try {
      recentLabels = await invoke<string[]>("get_recent_labels", { limit: 12 });
    } catch {
      recentLabels = [];
    }
  }

  function applyRecentLabel(index: number) {
    if (index < 0 || index >= recentLabels.length) return;
    historyIndex = index;
    text = recentLabels[index];
    requestAnimationFrame(() => {
      if (!textareaEl) return;
      const pos = textareaEl.value.length;
      textareaEl.focus();
      textareaEl.setSelectionRange(pos, pos);
    });
  }

  function cycleRecentLabels(direction: 1 | -1) {
    const total = recentLabels.length;
    if (total === 0) return;

    if (historyIndex === -1) {
      draftText = text;
      applyRecentLabel(direction === 1 ? 0 : total - 1);
      return;
    }

    if (direction === 1 && historyIndex === total - 1) {
      historyIndex = -1;
      text = draftText;
      requestAnimationFrame(() => {
        if (!textareaEl) return;
        const pos = textareaEl.value.length;
        textareaEl.focus();
        textareaEl.setSelectionRange(pos, pos);
      });
      return;
    }

    applyRecentLabel((historyIndex + direction + total) % total);
  }

  function onLabelInput() {
    draftText = text;
    historyIndex = -1;
  }

  function clearLabelText() {
    text = "";
    draftText = "";
    historyIndex = -1;
    requestAnimationFrame(() => textareaEl?.focus());
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape")                             { e.preventDefault(); closeWindow(); }
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); submit();      }
    if (
      (e.key === "ArrowUp" || e.key === "ArrowDown") &&
      document.activeElement === textareaEl &&
      !e.ctrlKey && !e.metaKey && !e.altKey
    ) {
      e.preventDefault();
      cycleRecentLabels(e.key === "ArrowDown" ? 1 : -1);
    }
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────
  onMount(() => {
    labelStartUtc = Math.floor(Date.now() / 1000);
    labelTitlebarState.active = true;
    timer = setInterval(() => elapsed++, 1000);
    setTimeout(() => textareaEl?.focus(), 60);
    loadRecentLabels();
  });
  onDestroy(() => {
    if (timer) clearInterval(timer);
    labelTitlebarState.active = false;
    labelTitlebarState.elapsed = "0s";
  });

  $effect(() => {
    labelTitlebarState.elapsed = fmtElapsed(elapsed);
  });

  const MAX_CHARS        = 1000;
  const MAX_CONTEXT_CHARS = 20_000;
  const nearLimit        = $derived(text.length > MAX_CHARS * 0.85);
  const overLimit        = $derived(text.length > MAX_CHARS);
  const canSubmit        = $derived(text.trim().length > 0 && !overLimit && !saving);

  useWindowTitle("window.title.label");
</script>

<svelte:window onkeydown={onKeydown} />

<main class="h-full min-h-0 bg-background text-foreground flex flex-col overflow-hidden">

  <!-- ── Body: label + context stacked, both flex-grow ───────────────────── -->
  <div class="flex flex-col px-4 pt-3 pb-1 gap-2 min-h-0 flex-1">

    <!-- Label textarea (shorter — the "tag") -->
    <Textarea
      bind:ref={textareaEl}
      bind:value={text}
      oninput={onLabelInput}
      placeholder={t("label.placeholder")}
      maxlength={MAX_CHARS}
      class="resize-none border-border dark:border-white/[0.09]
             bg-slate-50 dark:bg-[#111118]
             text-[0.82rem] leading-relaxed shrink-0"
      style="height: 80px"
    />

    {#if recentLabels.length > 0}
      <div class="flex flex-col gap-1.5 shrink-0">
        <div class="flex items-center justify-between gap-2">
          <span class="text-[0.58rem] font-semibold uppercase tracking-wider text-muted-foreground/60">
            {t("label.previousLabels")}
          </span>
          <span class="text-[0.56rem] text-muted-foreground/45 select-none">
            {t("label.previousHint")}
          </span>
        </div>
        <div class="flex flex-wrap gap-1.5 max-h-16 overflow-y-auto pr-0.5">
          {#each recentLabels as recentLabel, idx}
            <button
              type="button"
              class="max-w-full rounded-md border border-border dark:border-white/[0.1]
                     bg-muted/35 hover:bg-muted/60 px-2 py-0.5 text-[0.66rem]
                     text-foreground/85 truncate"
              onclick={() => applyRecentLabel(idx)}
              title={recentLabel}
            >
              {recentLabel}
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Context block (takes all remaining vertical space) -->
    <div class="flex flex-col gap-1 min-h-0 flex-1">
      <label for="label-context" class="text-[0.58rem] font-semibold uppercase tracking-wider
                    text-muted-foreground/60 shrink-0">
        {t("label.contextLabel")}
        <span class="font-normal normal-case tracking-normal text-muted-foreground/40 ml-1">
          — {t("label.contextHint")}
        </span>
      </label>
      <textarea
        id="label-context"
        bind:this={contextEl}
        bind:value={context}
        placeholder={t("label.contextPlaceholder")}
        maxlength={MAX_CONTEXT_CHARS}
        class="flex-1 min-h-0 rounded-md border border-border dark:border-white/[0.09]
               bg-slate-50 dark:bg-[#111118]
               px-3 py-1.5 text-[0.78rem] text-foreground leading-relaxed
               placeholder:text-muted-foreground/40
           focus:outline-none focus:ring-1 focus:ring-ring/50
               resize-none"
      ></textarea>
    </div>

  </div>

  <!-- ── Footer ───────────────────────────────────────────────────────────── -->
  <div class="flex items-center gap-3 px-4 pb-4 shrink-0">

    <span class="text-[0.62rem] tabular-nums shrink-0
                 {overLimit ? 'text-destructive font-semibold'
                 : nearLimit ? 'text-amber-500 dark:text-amber-400'
                 : 'text-muted-foreground/50'}">
      {text.length}/{MAX_CHARS}
    </span>

    <span class="text-[0.6rem] text-muted-foreground/35 select-none">
      {t("label.saveHint", { key: isMac ? "⌘" : "Ctrl" })}
    </span>

    {#if error}
      <span class="text-[0.65rem] text-destructive truncate flex-1 text-right">{error}</span>
    {:else}
      <span class="flex-1"></span>
    {/if}

    <div class="flex gap-2 shrink-0">
          <Button variant="outline" size="sm" class="text-[0.72rem] h-7 px-3"
            disabled={text.length === 0} onclick={clearLabelText}>{t("common.clear")}</Button>
      <Button variant="outline" size="sm" class="text-[0.72rem] h-7 px-3"
              onclick={closeWindow}>{t("common.cancel")}</Button>
      <Button size="sm" class="text-[0.72rem] h-7 px-3"
              disabled={!canSubmit} onclick={submit}>
        {saving ? t("common.saving") : t("label.saveLabel")}
      </Button>
    </div>
  </div>

</main>
