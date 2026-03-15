<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { Button } from "$lib/components/ui/button";
  import { Card, CardContent } from "$lib/components/ui/card";
  import { t } from "$lib/i18n/index.svelte";
  import { fmtDateTimeLocale } from "$lib/format";

  interface HookRule {
    name: string;
    enabled: boolean;
    keywords: string[];
    scenario: string;
    command: string;
    text: string;
    distance_threshold: number;
    recent_limit: number;
  }

  interface HookLastTrigger {
    triggered_at_utc: number;
    distance: number;
    label_id: number | null;
    label_text: string | null;
    label_eeg_start_utc: number | null;
  }

  interface HookStatus {
    hook: HookRule;
    last_trigger: HookLastTrigger | null;
  }

  interface HookDistanceSuggestion {
    label_n: number;
    ref_n: number;
    sample_n: number;
    eeg_min: number;
    eeg_p25: number;
    eeg_p50: number;
    eeg_p75: number;
    eeg_max: number;
    suggested: number;
    note: string;
  }

  interface HookLogRow {
    id: number;
    triggered_at_utc: number;
    hook_json: string;
    trigger_json: string;
    payload_json: string;
  }

  interface HookKeywordSuggestion {
    keyword: string;
    source: string;
    score: number;
  }

  const NEW_HOOK: HookRule = {
    name: "",
    enabled: true,
    keywords: [],
    scenario: "any",
    command: "",
    text: "",
    distance_threshold: 0.1,
    recent_limit: 12,
  };

  interface HookExample {
    id: string;
    name: string;
    scenario: string;
    keywords: string[];
    command: string;
    text: string;
    distance_threshold: number;
    recent_limit: number;
  }

  const HOOK_EXAMPLES: HookExample[] = [
    {
      id: "cognitive-focus-reset",
      name: "Deep Work Guard",
      scenario: "cognitive",
      keywords: ["focus", "deep work", "flow"],
      command: "focus_reset",
      text: "Cognitive load is high. Take a 2-minute reset and resume.",
      distance_threshold: 0.14,
      recent_limit: 12,
    },
    {
      id: "emotional-calm",
      name: "Calm Recovery",
      scenario: "emotional",
      keywords: ["stress", "anxious", "overwhelmed"],
      command: "calm_breath",
      text: "Emotional strain detected. Slow breathing: inhale 4, exhale 6 for 1 minute.",
      distance_threshold: 0.16,
      recent_limit: 12,
    },
    {
      id: "physical-break",
      name: "Body Break",
      scenario: "physical",
      keywords: ["fatigue", "slump", "tired"],
      command: "micro_break",
      text: "Physical fatigue detected. Stand up, stretch, and hydrate for 90 seconds.",
      distance_threshold: 0.18,
      recent_limit: 12,
    },
  ];

  let hooks = $state<HookRule[]>([]);
  let statuses = $state<Record<string, HookLastTrigger | null>>({});
  let keywordDrafts = $state<string[]>([]);
  let loading = $state(true);
  let saving = $state(false);
  let saved = $state(false);
  let openingSession = $state<string | null>(null);

  // ── Relative timer ─────────────────────────────────────────────────────────
  let nowSecs = $state(Math.floor(Date.now() / 1000));

  // ── Distance suggestion ────────────────────────────────────────────────────
  let suggestions = $state<Record<number, HookDistanceSuggestion | null>>({});
  let suggestingIdx = $state<number | null>(null);

  // ── Keyword suggestions ───────────────────────────────────────────────────
  let keywordSuggestions = $state<Record<number, HookKeywordSuggestion[]>>({});
  let keywordSuggesting = $state<Record<number, boolean>>({});
  let keywordSuggestionFocus = $state<Record<number, number>>({});
  const keywordSuggestDebounce = new Map<number, ReturnType<typeof setTimeout>>();

  // ── History log ───────────────────────────────────────────────────────────
  let logRows = $state<HookLogRow[]>([]);
  let logTotal = $state(0);
  let logOffset = $state(0);
  let logLoading = $state(false);
  let showLog = $state(false);
  const LOG_PAGE = 20;

  async function loadHooks() {
    loading = true;
    try {
      hooks = await invoke<HookRule[]>("get_hooks");
      keywordDrafts = hooks.map(() => "");
      suggestions = {};
      keywordSuggestions = {};
      keywordSuggesting = {};
      keywordSuggestionFocus = {};
      await loadStatuses();
    } catch {
      hooks = [];
      keywordDrafts = [];
      statuses = {};
    } finally {
      loading = false;
    }
  }

  async function loadStatuses() {
    try {
      const rows = await invoke<HookStatus[]>("get_hook_statuses");
      const next: Record<string, HookLastTrigger | null> = {};
      for (const row of rows) next[row.hook.name] = row.last_trigger;
      statuses = next;
    } catch {
      statuses = {};
    }
  }

  async function loadLog() {
    logLoading = true;
    try {
      logTotal = await invoke<number>("get_hook_log_count");
      logRows = await invoke<HookLogRow[]>("get_hook_log", { limit: LOG_PAGE, offset: logOffset });
    } catch {
      logRows = [];
    } finally {
      logLoading = false;
    }
  }

  async function suggestDistances(i: number) {
    const kws = hooks[i]?.keywords ?? [];
    if (kws.length === 0) return;
    suggestingIdx = i;
    try {
      const result = await invoke<HookDistanceSuggestion>("suggest_hook_distances", { keywords: kws });
      suggestions = { ...suggestions, [i]: result };
    } catch {
      suggestions = { ...suggestions, [i]: null };
    } finally {
      suggestingIdx = null;
    }
  }

  function applySuggestion(i: number) {
    const s = suggestions[i];
    if (!s) return;
    updateHook(i, { distance_threshold: s.suggested });
    suggestions = { ...suggestions, [i]: null };
  }

  function tsToUnix(tsUtc: number): number {
    const s = String(tsUtc).padStart(14, "0");
    const y = Number(s.slice(0, 4));
    const m = Number(s.slice(4, 6)) - 1;
    const d = Number(s.slice(6, 8));
    const hh = Number(s.slice(8, 10));
    const mm = Number(s.slice(10, 12));
    const ss = Number(s.slice(12, 14));
    return Math.floor(Date.UTC(y, m, d, hh, mm, ss) / 1000);
  }

  function fmtUtc(tsUtc: number): string {
    const unix = tsToUnix(tsUtc);
    if (!unix || Number.isNaN(unix)) return "—";
    return fmtDateTimeLocale(unix);
  }

  function relativeAge(tsUtc: number): string {
    const unix = tsToUnix(tsUtc);
    if (!unix || Number.isNaN(unix)) return "";
    const diff = nowSecs - unix;
    if (diff < 0) return "";
    if (diff < 60) return `${diff}s ${t("hooks.ago")}`;
    if (diff < 3600) return `${Math.floor(diff / 60)}m ${t("hooks.ago")}`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ${t("hooks.ago")}`;
    return `${Math.floor(diff / 86400)}d ${t("hooks.ago")}`;
  }

  async function openTriggeredSession(hookName: string, trigger: HookLastTrigger | null) {
    const ts = trigger?.label_eeg_start_utc;
    if (!ts) return;
    openingSession = hookName;
    try {
      await invoke("open_session_for_timestamp", { timestampUtc: ts });
    } finally {
      openingSession = null;
    }
  }

  onMount(() => {
    loadHooks();
    const statusTimer = setInterval(loadStatuses, 5000);
    const clockTimer  = setInterval(() => { nowSecs = Math.floor(Date.now() / 1000); }, 1000);
    return () => {
      clearInterval(statusTimer);
      clearInterval(clockTimer);
      for (const timer of keywordSuggestDebounce.values()) clearTimeout(timer);
      keywordSuggestDebounce.clear();
    };
  });

  function addHook() {
    hooks = [...hooks, { ...NEW_HOOK }];
    keywordDrafts = [...keywordDrafts, ""];
  }

  function removeHook(i: number) {
    hooks = hooks.filter((_, idx) => idx !== i);
    keywordDrafts = keywordDrafts.filter((_, idx) => idx !== i);
    const next = { ...suggestions };
    delete next[i];
    suggestions = next;
    const nextK = { ...keywordSuggestions };
    delete nextK[i];
    keywordSuggestions = nextK;
    const nextS = { ...keywordSuggesting };
    delete nextS[i];
    keywordSuggesting = nextS;
    const nextF = { ...keywordSuggestionFocus };
    delete nextF[i];
    keywordSuggestionFocus = nextF;
    const t = keywordSuggestDebounce.get(i);
    if (t) {
      clearTimeout(t);
      keywordSuggestDebounce.delete(i);
    }
  }

  function updateHook(i: number, patch: Partial<HookRule>) {
    hooks = hooks.map((h, idx) => idx === i ? { ...h, ...patch } : h);
  }

  function addKeywordValue(i: number, value: string) {
    const kw = value.trim();
    if (!kw) return;
    const exists = hooks[i].keywords.some((x) => x.toLowerCase() === kw.toLowerCase());
    if (exists) {
      keywordDrafts = keywordDrafts.map((v, idx) => idx === i ? "" : v);
      keywordSuggestions = { ...keywordSuggestions, [i]: [] };
      return;
    }
    const next = [...hooks[i].keywords, kw];
    updateHook(i, { keywords: next });
    keywordDrafts = keywordDrafts.map((v, idx) => idx === i ? "" : v);
    keywordSuggestions = { ...keywordSuggestions, [i]: [] };
  }

  function addKeyword(i: number) {
    addKeywordValue(i, keywordDrafts[i] ?? "");
  }

  function removeKeyword(hookIndex: number, keywordIndex: number) {
    updateHook(hookIndex, {
      keywords: hooks[hookIndex].keywords.filter((_, idx) => idx !== keywordIndex),
    });
  }

  function sourceText(source: string): string {
    if (source === "both") return t("hooks.suggestionSourceBoth");
    if (source === "embedding") return t("hooks.suggestionSourceEmbedding");
    return t("hooks.suggestionSourceFuzzy");
  }

  async function fetchKeywordSuggestions(i: number) {
    const draft = (keywordDrafts[i] ?? "").trim();
    if (draft.length < 2) {
      keywordSuggestions = { ...keywordSuggestions, [i]: [] };
      keywordSuggesting = { ...keywordSuggesting, [i]: false };
      return;
    }
    keywordSuggesting = { ...keywordSuggesting, [i]: true };
    try {
      const rows = await invoke<HookKeywordSuggestion[]>("suggest_hook_keywords", { draft, limit: 8 });
      const existing = new Set((hooks[i]?.keywords ?? []).map((x) => x.toLowerCase()));
      keywordSuggestions = {
        ...keywordSuggestions,
        [i]: rows.filter((r) => !existing.has(r.keyword.toLowerCase())),
      };
      keywordSuggestionFocus = { ...keywordSuggestionFocus, [i]: 0 };
    } catch {
      keywordSuggestions = { ...keywordSuggestions, [i]: [] };
      keywordSuggestionFocus = { ...keywordSuggestionFocus, [i]: -1 };
    } finally {
      keywordSuggesting = { ...keywordSuggesting, [i]: false };
    }
  }

  function onKeywordInputKeyDown(i: number, e: KeyboardEvent) {
    const suggestionsForRow = keywordSuggestions[i] ?? [];
    if (e.key === "ArrowDown") {
      if (suggestionsForRow.length === 0) return;
      e.preventDefault();
      const current = keywordSuggestionFocus[i] ?? -1;
      const next = current < 0 ? 0 : (current + 1) % suggestionsForRow.length;
      keywordSuggestionFocus = { ...keywordSuggestionFocus, [i]: next };
      return;
    }
    if (e.key === "ArrowUp") {
      if (suggestionsForRow.length === 0) return;
      e.preventDefault();
      const current = keywordSuggestionFocus[i] ?? 0;
      const next = current <= 0 ? suggestionsForRow.length - 1 : current - 1;
      keywordSuggestionFocus = { ...keywordSuggestionFocus, [i]: next };
      return;
    }
    if (e.key === "Escape") {
      keywordSuggestions = { ...keywordSuggestions, [i]: [] };
      keywordSuggestionFocus = { ...keywordSuggestionFocus, [i]: -1 };
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      const idx = keywordSuggestionFocus[i] ?? -1;
      const active = idx >= 0 ? suggestionsForRow[idx] : null;
      if (active) {
        applyKeywordSuggestion(i, active.keyword);
      } else {
        addKeyword(i);
      }
    }
  }

  function addScenarioExample(example: HookExample) {
    hooks = [...hooks, {
      name: example.name,
      enabled: true,
      keywords: [...example.keywords],
      scenario: example.scenario,
      command: example.command,
      text: example.text,
      distance_threshold: example.distance_threshold,
      recent_limit: example.recent_limit,
    }];
    keywordDrafts = [...keywordDrafts, ""];
  }

  function onKeywordDraftInput(i: number, value: string) {
    keywordDrafts = keywordDrafts.map((v, idx) => idx === i ? value : v);
    const prev = keywordSuggestDebounce.get(i);
    if (prev) clearTimeout(prev);
    const timer = setTimeout(() => { fetchKeywordSuggestions(i); }, 180);
    keywordSuggestDebounce.set(i, timer);
  }

  function applyKeywordSuggestion(i: number, kw: string) {
    addKeywordValue(i, kw);
  }

  async function saveHooks() {
    saving = true;
    saved = false;
    try {
      await invoke("set_hooks", { hooks });
      saved = true;
      setTimeout(() => { saved = false; }, 1500);
      await loadHooks();
    } finally {
      saving = false;
    }
  }

  // ── Log parsing helpers ───────────────────────────────────────────────────
  function parseJson(s: string): Record<string, unknown> {
    try { return JSON.parse(s); } catch { return {}; }
  }

  function logHookName(row: HookLogRow): string {
    const h = parseJson(row.hook_json);
    return String(h.name ?? "?");
  }

  function logLabel(row: HookLogRow): string {
    const t = parseJson(row.trigger_json);
    return String(t.label_text ?? "");
  }

  function logDistance(row: HookLogRow): string {
    const t = parseJson(row.trigger_json);
    const d = Number(t.distance);
    return Number.isFinite(d) ? d.toFixed(3) : "—";
  }
</script>

<Card class="border border-border/50 bg-background/95">
  <CardContent class="space-y-4 p-4">
    <div class="flex items-center justify-between gap-3">
      <div>
        <h3 class="text-sm font-semibold text-foreground">{t("settingsTabs.hooks")}</h3>
        <p class="text-xs text-muted-foreground">{t("hooks.subtitle")}</p>
      </div>
      <div class="flex items-center gap-2">
        <Button variant="outline" size="sm" onclick={addHook}>{t("hooks.addHook")}</Button>
        <Button size="sm" onclick={saveHooks} disabled={saving || loading}>
          {saving ? t("hooks.saving") : saved ? t("hooks.saved") : t("hooks.save")}
        </Button>
      </div>
    </div>

    <div class="rounded-md border border-border/60 bg-card/40 p-2 space-y-1.5">
      <div class="text-[0.67rem] font-medium text-muted-foreground">{t("hooks.examplesTitle")}</div>
      <div class="flex flex-wrap gap-1.5">
        {#each HOOK_EXAMPLES as ex}
          <button
            class="inline-flex items-center gap-1 rounded-md border border-border/60 px-2 py-1 text-[0.65rem] text-foreground hover:bg-muted"
            onclick={() => addScenarioExample(ex)}
            title={t("hooks.examplesApply")}
          >
            <span>{ex.name}</span>
            <span class="text-muted-foreground">· {t(`hooks.scenario.${ex.scenario}`)}</span>
          </button>
        {/each}
      </div>
    </div>

    {#if loading}
      <p class="text-xs text-muted-foreground">{t("hooks.loading")}</p>
    {:else if hooks.length === 0}
      <p class="text-xs text-muted-foreground">{t("hooks.empty")}</p>
    {:else}
      <div class="space-y-3">
        {#each hooks as hook, i}
          <div class="rounded-lg border border-border/60 bg-card/50 p-3 space-y-3">
            <div class="grid grid-cols-12 gap-2 items-center">
              <input
                class="col-span-7 rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground"
                placeholder={t("hooks.name")}
                value={hook.name}
                oninput={(e) => updateHook(i, { name: (e.currentTarget as HTMLInputElement).value })}
              />
              <label class="col-span-3 flex items-center gap-2 text-xs text-muted-foreground">
                <input
                  type="checkbox"
                  checked={hook.enabled}
                  onchange={(e) => updateHook(i, { enabled: (e.currentTarget as HTMLInputElement).checked })}
                />
                {t("hooks.enabled")}
              </label>
              <Button class="col-span-2 min-w-0 w-full h-auto whitespace-normal break-words py-1.5 text-[0.67rem] leading-tight" variant="outline" size="sm" onclick={() => removeHook(i)}>
                {t("hooks.remove")}
              </Button>
            </div>

            {#if statuses[hook.name]}
              <div class="grid grid-cols-12 gap-2 items-center rounded-md border border-border/50 bg-muted/30 p-2">
                <div class="col-span-9 text-[0.68rem] text-muted-foreground">
                  <span class="font-medium text-foreground">{t("hooks.lastTrigger")}</span>
                  <span class="ml-1">{fmtUtc(statuses[hook.name]?.triggered_at_utc ?? 0)}</span>
                  {#if statuses[hook.name]?.triggered_at_utc}
                    <span class="ml-1 text-primary/70">({relativeAge(statuses[hook.name]!.triggered_at_utc)})</span>
                  {/if}
                  {#if statuses[hook.name]?.label_text}
                    <span class="ml-2">• {statuses[hook.name]?.label_text}</span>
                  {/if}
                  <span class="ml-2">• d={Number(statuses[hook.name]?.distance ?? 0).toFixed(3)}</span>
                </div>
                <div class="col-span-3 flex justify-end">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={openingSession === hook.name || !statuses[hook.name]?.label_eeg_start_utc}
                    onclick={() => openTriggeredSession(hook.name, statuses[hook.name] ?? null)}
                  >
                    {openingSession === hook.name ? t("hooks.opening") : t("hooks.openSession")}
                  </Button>
                </div>
              </div>
            {/if}

            <div class="grid grid-cols-12 gap-2 items-center">
              <input
                class="col-span-9 rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground"
                placeholder={t("hooks.addKeywordPlaceholder")}
                value={keywordDrafts[i] ?? ""}
                oninput={(e) => onKeywordDraftInput(i, (e.currentTarget as HTMLInputElement).value)}
                onkeydown={(e) => onKeywordInputKeyDown(i, e)}
              />
              <Button class="col-span-3 min-w-0 w-full h-auto whitespace-normal break-words py-1.5 text-[0.67rem] leading-tight" variant="outline" size="sm" onclick={() => addKeyword(i)}>
                {t("hooks.addKeyword")}
              </Button>
            </div>

            {#if (keywordDrafts[i] ?? "").trim().length >= 2}
              <div class="rounded-md border border-border/60 bg-card/60 p-2 space-y-1">
                <div class="text-[0.62rem] font-medium text-muted-foreground">{t("hooks.keywordSuggestions")}</div>
                <div class="text-[0.6rem] text-muted-foreground">{t("hooks.keywordSuggestionKeyboardHint")}</div>
                {#if keywordSuggesting[i]}
                  <div class="text-[0.65rem] text-muted-foreground">{t("hooks.keywordSuggestionsLoading")}</div>
                {:else if (keywordSuggestions[i]?.length ?? 0) === 0}
                  <div class="text-[0.65rem] text-muted-foreground">{t("hooks.keywordSuggestionEmpty")}</div>
                {:else}
                  <div class="flex flex-wrap gap-1.5">
                    {#each keywordSuggestions[i] ?? [] as s, sIdx}
                      <button
                        class="inline-flex items-center gap-1 rounded-md border px-2 py-1 text-[0.65rem] text-foreground hover:bg-muted {sIdx === (keywordSuggestionFocus[i] ?? -1) ? 'border-primary/70 bg-primary/10' : 'border-border/60'}"
                        onclick={() => applyKeywordSuggestion(i, s.keyword)}
                        onmouseenter={() => keywordSuggestionFocus = { ...keywordSuggestionFocus, [i]: sIdx }}
                      >
                        <span>{s.keyword}</span>
                        <span class="text-muted-foreground">· {sourceText(s.source)}</span>
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>
            {/if}

            <div class="grid grid-cols-12 gap-2 items-center">
              <span class="col-span-4 text-xs text-muted-foreground">{t("hooks.scenarioLabel")}</span>
              <div class="col-span-8 relative">
                <select
                  class="w-full appearance-none rounded-md border border-border bg-background px-2 py-1.5 pr-7 text-xs text-foreground ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                  value={hook.scenario ?? "any"}
                  onchange={(e) => updateHook(i, { scenario: (e.currentTarget as HTMLSelectElement).value })}
                >
                  <option value="any">{t("hooks.scenario.any")}</option>
                  <option value="cognitive">{t("hooks.scenario.cognitive")}</option>
                  <option value="emotional">{t("hooks.scenario.emotional")}</option>
                  <option value="physical">{t("hooks.scenario.physical")}</option>
                </select>
                <svg class="pointer-events-none absolute right-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
                  <path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clip-rule="evenodd" />
                </svg>
              </div>
            </div>

            {#if hook.keywords.length > 0}
              <div class="flex flex-wrap gap-2">
                {#each hook.keywords as kw, kIdx}
                  <button
                    class="inline-flex items-center gap-1 rounded-md border border-border/60 px-2 py-1 text-[0.67rem] text-foreground hover:bg-muted"
                    onclick={() => removeKeyword(i, kIdx)}
                    title={t("hooks.removeKeyword")}
                  >
                    <span>{kw}</span>
                    <span aria-hidden="true">×</span>
                  </button>
                {/each}
              </div>
            {/if}

            <div class="grid grid-cols-12 gap-2">
              <input
                class="col-span-6 rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground"
                placeholder={t("hooks.command")}
                value={hook.command}
                oninput={(e) => updateHook(i, { command: (e.currentTarget as HTMLInputElement).value })}
              />
              <input
                class="col-span-6 rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground"
                placeholder={t("hooks.text")}
                value={hook.text}
                oninput={(e) => updateHook(i, { text: (e.currentTarget as HTMLInputElement).value })}
              />
            </div>

            <!-- Distance threshold row + suggest button -->
            <div class="grid grid-cols-12 gap-2 items-center">
              <label class="col-span-5 flex items-center gap-2 text-xs text-muted-foreground">
                {t("hooks.distance")}
                <input
                  type="number"
                  min="0.01"
                  max="1"
                  step="0.01"
                  class="w-20 rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground"
                  value={hook.distance_threshold}
                  oninput={(e) => updateHook(i, { distance_threshold: Number((e.currentTarget as HTMLInputElement).value || 0.1) })}
                />
              </label>
              <div class="col-span-4 flex items-center">
                <Button
                  variant="outline"
                  size="sm"
                  class="text-[0.67rem]"
                  disabled={hook.keywords.length === 0 || suggestingIdx === i}
                  onclick={() => suggestDistances(i)}
                >
                  {suggestingIdx === i ? t("hooks.suggesting") : t("hooks.suggest")}
                </Button>
              </div>
              <label class="col-span-3 flex items-center gap-2 text-xs text-muted-foreground">
                {t("hooks.recent")}
                <input
                  type="number"
                  min="10"
                  max="20"
                  step="1"
                  class="w-16 rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground"
                  value={hook.recent_limit}
                  oninput={(e) => updateHook(i, { recent_limit: Number((e.currentTarget as HTMLInputElement).value || 12) })}
                />
              </label>
            </div>

            <!-- Suggestion result panel -->
            {#if suggestions[i]}
              {@const sug = suggestions[i]!}
              <div class="rounded-md border border-primary/30 bg-primary/5 p-3 space-y-2 text-[0.68rem]">
                <div class="flex items-center justify-between gap-2">
                  <span class="font-medium text-foreground">{t("hooks.suggestResult")}</span>
                  <div class="flex items-center gap-2">
                    <span class="text-primary font-semibold">{t("hooks.suggestThreshold")} {sug.suggested.toFixed(2)}</span>
                    <Button size="sm" class="h-6 px-2 text-[0.67rem]" onclick={() => applySuggestion(i)}>
                      {t("hooks.applyThreshold")}
                    </Button>
                    <button
                      class="text-muted-foreground hover:text-foreground"
                      onclick={() => { suggestions = { ...suggestions, [i]: null }; }}
                      aria-label="dismiss"
                    >×</button>
                  </div>
                </div>
                <!-- Percentile bar -->
                {#if sug.eeg_max > 0}
                  <div class="space-y-1">
                    <div class="relative h-4 rounded bg-muted overflow-hidden">
                      <!-- p25 zone (closest matches) -->
                      <div
                        class="absolute top-0 bottom-0 bg-primary/35"
                        style="left: 0%; width: {Math.min((sug.eeg_p25 / sug.eeg_max) * 100, 100)}%"
                      ></div>
                      <!-- p25–p50 zone -->
                      <div
                        class="absolute top-0 bottom-0 bg-primary/22"
                        style="left: {(sug.eeg_p25 / sug.eeg_max) * 100}%; width: {((sug.eeg_p50 - sug.eeg_p25) / sug.eeg_max) * 100}%"
                      ></div>
                      <!-- p50–p75 zone -->
                      <div
                        class="absolute top-0 bottom-0 bg-primary/12"
                        style="left: {(sug.eeg_p50 / sug.eeg_max) * 100}%; width: {((sug.eeg_p75 - sug.eeg_p50) / sug.eeg_max) * 100}%"
                      ></div>
                      <!-- current threshold marker -->
                      <div
                        class="absolute top-0 bottom-0 w-0.5 bg-primary"
                        style="left: {Math.min((hook.distance_threshold / sug.eeg_max) * 100, 100)}%"
                      ></div>
                      <!-- suggested threshold marker -->
                      <div
                        class="absolute top-0 bottom-0 w-0.5 bg-primary"
                        style="left: {Math.min((sug.suggested / sug.eeg_max) * 100, 100)}%"
                      ></div>
                    </div>
                    <div class="flex justify-between text-[0.62rem] text-muted-foreground">
                      <span>{t("hooks.distMin")} {sug.eeg_min.toFixed(3)}</span>
                      <span>p25 {sug.eeg_p25.toFixed(3)}</span>
                      <span>p50 {sug.eeg_p50.toFixed(3)}</span>
                      <span>p75 {sug.eeg_p75.toFixed(3)}</span>
                      <span>{t("hooks.distMax")} {sug.eeg_max.toFixed(3)}</span>
                    </div>
                  </div>
                {/if}
                <p class="text-muted-foreground leading-snug">{sug.note}</p>
                <p class="text-muted-foreground">
                  {t("hooks.suggestSamples", { label_n: sug.label_n, ref_n: sug.ref_n, sample_n: sug.sample_n })}
                </p>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <!-- ── Hook fire history ──────────────────────────────────────────────── -->
    <div class="border-t border-border/40 pt-3">
      <button
        class="flex w-full items-center gap-2 text-xs font-medium text-muted-foreground hover:text-foreground"
        onclick={() => { showLog = !showLog; if (showLog && logRows.length === 0) loadLog(); }}
      >
        <svg class="h-3.5 w-3.5 transition-transform {showLog ? 'rotate-90' : ''}" viewBox="0 0 20 20" fill="currentColor">
          <path fill-rule="evenodd" d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z" clip-rule="evenodd" />
        </svg>
        {t("hooks.history")}
        {#if logTotal > 0}
          <span class="ml-1 rounded-full bg-muted px-1.5 py-0.5 text-[0.62rem]">{logTotal}</span>
        {/if}
      </button>

      {#if showLog}
        <div class="mt-3 space-y-2">
          {#if logLoading}
            <p class="text-xs text-muted-foreground">{t("hooks.historyLoading")}</p>
          {:else if logRows.length === 0}
            <p class="text-xs text-muted-foreground">{t("hooks.historyEmpty")}</p>
          {:else}
            <div class="space-y-1.5">
              {#each logRows as row}
                {@const hook_obj = parseJson(row.hook_json)}
                {@const trigger_obj = parseJson(row.trigger_json)}
                {@const payload_obj = parseJson(row.payload_json)}
                <details class="group rounded-md border border-border/50 bg-card/40">
                  <summary class="flex cursor-pointer items-center gap-2 p-2 text-[0.68rem]">
                    <span class="font-medium text-foreground">{logHookName(row)}</span>
                    <span class="text-muted-foreground">{fmtUtc(row.triggered_at_utc)}</span>
                    {#if logLabel(row)}
                      <span class="text-muted-foreground">• {logLabel(row)}</span>
                    {/if}
                    <span class="ml-auto text-muted-foreground">d={logDistance(row)}</span>
                  </summary>
                  <div class="border-t border-border/40 p-2 space-y-1 text-[0.65rem] text-muted-foreground">
                    <div class="grid grid-cols-2 gap-x-4 gap-y-0.5">
                      {#if trigger_obj.label_id !== undefined}
                        <span>{t("hooks.logLabelId")}: {trigger_obj.label_id}</span>
                      {/if}
                      {#if trigger_obj.distance !== undefined}
                        <span>{t("hooks.logDistance")}: {Number(trigger_obj.distance).toFixed(4)}</span>
                      {/if}
                      {#if payload_obj.command}
                        <span class="col-span-2">{t("hooks.logCommand")}: <code class="font-mono">{payload_obj.command}</code></span>
                      {/if}
                      {#if payload_obj.text}
                        <span class="col-span-2">{t("hooks.logText")}: {payload_obj.text}</span>
                      {/if}
                      {#if hook_obj.keywords && Array.isArray(hook_obj.keywords)}
                        <span class="col-span-2">{t("hooks.logKeywords")}: {(hook_obj.keywords as string[]).join(", ")}</span>
                      {/if}
                      {#if hook_obj.distance_threshold !== undefined}
                        <span>{t("hooks.logThresholdAtFire")}: {Number(hook_obj.distance_threshold).toFixed(3)}</span>
                      {/if}
                    </div>
                  </div>
                </details>
              {/each}
            </div>
            <!-- Pagination -->
            <div class="flex items-center justify-between pt-1">
              <Button
                variant="outline" size="sm"
                disabled={logOffset === 0}
                onclick={() => { logOffset = Math.max(0, logOffset - LOG_PAGE); loadLog(); }}
              >{t("hooks.logPrev")}</Button>
              <span class="text-[0.68rem] text-muted-foreground">
                {logOffset + 1}–{Math.min(logOffset + LOG_PAGE, logTotal)} / {logTotal}
              </span>
              <Button
                variant="outline" size="sm"
                disabled={logOffset + LOG_PAGE >= logTotal}
                onclick={() => { logOffset += LOG_PAGE; loadLog(); }}
              >{t("hooks.logNext")}</Button>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </CardContent>
</Card>
