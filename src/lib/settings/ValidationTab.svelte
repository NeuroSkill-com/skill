<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Validation & Research settings tab.

  All persistent state lives in the daemon at ~/.skill/validation.sqlite.
  This component is a thin reader/writer over the /v1/validation/* HTTP
  endpoints.  Each toggle PATCHes one field; the daemon validates and
  persists.  Documented in extensions/vscode/README.md "Validation roadmap".
-->
<script lang="ts">
import { onDestroy, onMount } from "svelte";
import { Button } from "$lib/components/ui/button";
import { CardContent } from "$lib/components/ui/card";
import { SectionHeader } from "$lib/components/ui/section-header";
import { SettingsCard } from "$lib/components/ui/settings-card";
import { ToggleRow } from "$lib/components/ui/toggle-row";
import { daemonGet, daemonPatch, daemonPost } from "$lib/daemon/http";
import { t } from "$lib/i18n/index.svelte";
import PvtPanel from "./PvtPanel.svelte";
import TlxForm from "./TlxForm.svelte";

// ── Types — must match crates/skill-data/src/validation_store.rs ─────────

interface KssConfig {
  enabled: boolean;
  max_per_day: number;
  min_interval_min: number;
  trigger_break_coach: boolean;
  trigger_random: boolean;
  random_weight: number;
}

interface TlxConfig {
  enabled: boolean;
  max_per_day: number;
  min_task_min: number;
  end_of_day: boolean;
}

interface PvtConfig {
  enabled: boolean;
  weekly_reminder: boolean;
  auto_fire: boolean;
}

interface EegFatigueConfig {
  enabled: boolean;
  window_secs: number;
}

interface ValidationConfig {
  respect_flow: boolean;
  quiet_before_hour: number;
  quiet_after_hour: number;
  kss: KssConfig;
  tlx: TlxConfig;
  pvt: PvtConfig;
  eeg_fatigue: EegFatigueConfig;
}

interface FatigueIndex {
  fatigue_idx: number | null;
  formula: string;
  reference: string;
}

interface KssRow {
  id: number;
  score: number;
  triggered_by: string;
  ts: number;
}

interface ResultsResponse {
  kss: KssRow[];
}

// ── State ────────────────────────────────────────────────────────────────

let config = $state<ValidationConfig | null>(null);
let loadError = $state<string | null>(null);
let saveStatus = $state<"idle" | "saving" | "saved" | "error">("idle");
let saveError = $state("");

let fatigueIndex = $state<number | null>(null);
let recentKssCount = $state(0);

let pvtOpen = $state(false);
let tlxOpen = $state(false);

let pollTimer: ReturnType<typeof setInterval> | undefined;

// ── Mount / unmount ──────────────────────────────────────────────────────

onMount(async () => {
  await loadConfig();
  await refreshResults();
  pollTimer = setInterval(() => void refreshFatigue(), 5_000);
});

onDestroy(() => {
  if (pollTimer) clearInterval(pollTimer);
});

async function loadConfig() {
  try {
    config = await daemonGet<ValidationConfig>("/v1/validation/config");
  } catch (e) {
    loadError = String(e);
  }
}

async function refreshResults() {
  try {
    const since = Math.floor(Date.now() / 1000) - 7 * 86_400;
    const r = await daemonGet<ResultsResponse>(`/v1/validation/results?since=${since}`);
    recentKssCount = r?.kss?.length ?? 0;
  } catch {
    /* non-fatal */
  }
}

async function refreshFatigue() {
  try {
    const r = await daemonGet<FatigueIndex>("/v1/validation/fatigue-index");
    fatigueIndex = r?.fatigue_idx ?? null;
  } catch {
    fatigueIndex = null;
  }
}

// ── Patch helpers ────────────────────────────────────────────────────────

async function patch(body: Record<string, unknown>) {
  if (!config) return;
  saveStatus = "saving";
  saveError = "";
  try {
    config = await daemonPatch<ValidationConfig>("/v1/validation/config", body);
    saveStatus = "saved";
    setTimeout(() => {
      if (saveStatus === "saved") saveStatus = "idle";
    }, 1500);
  } catch (e) {
    saveStatus = "error";
    saveError = String(e);
  }
}

// ── Toggle handlers ──────────────────────────────────────────────────────

function toggleRespectFlow() {
  if (!config) return;
  void patch({ respect_flow: !config.respect_flow });
}

function toggleKss() {
  if (!config) return;
  void patch({ kss: { enabled: !config.kss.enabled } });
}

function toggleKssBreakCoach() {
  if (!config) return;
  void patch({ kss: { trigger_break_coach: !config.kss.trigger_break_coach } });
}

function toggleKssRandom() {
  if (!config) return;
  void patch({ kss: { trigger_random: !config.kss.trigger_random } });
}

function toggleTlx() {
  if (!config) return;
  void patch({ tlx: { enabled: !config.tlx.enabled } });
}

function toggleTlxEndOfDay() {
  if (!config) return;
  void patch({ tlx: { end_of_day: !config.tlx.end_of_day } });
}

function togglePvt() {
  if (!config) return;
  void patch({ pvt: { enabled: !config.pvt.enabled } });
}

function togglePvtWeekly() {
  if (!config) return;
  void patch({ pvt: { weekly_reminder: !config.pvt.weekly_reminder } });
}

function toggleEegFatigue() {
  if (!config) return;
  void patch({ eeg_fatigue: { enabled: !config.eeg_fatigue.enabled } });
}

// ── Numeric setters (debounced via blur) ─────────────────────────────────

function bindNumber(channel: "kss" | "tlx" | "pvt" | "eeg_fatigue", field: string) {
  return (e: Event) => {
    const target = e.target as HTMLInputElement;
    const v = Number(target.value);
    if (!Number.isFinite(v)) return;
    void patch({ [channel]: { [field]: v } });
  };
}

function setQuietBefore(e: Event) {
  const v = Number((e.target as HTMLInputElement).value);
  if (!Number.isFinite(v) || v < 0 || v > 23) return;
  void patch({ quiet_before_hour: v });
}

function setQuietAfter(e: Event) {
  const v = Number((e.target as HTMLInputElement).value);
  if (!Number.isFinite(v) || v < 0 || v > 23) return;
  void patch({ quiet_after_hour: v });
}

// ── Calibration Week ─────────────────────────────────────────────────────

async function startCalibrationWeek() {
  // Aggressive presets, all four channels for one week.
  await daemonPatch("/v1/validation/config", {
    kss: { enabled: true, max_per_day: 8, min_interval_min: 60, trigger_random: true, random_weight: 0.5 },
    tlx: { enabled: true, max_per_day: 6, min_task_min: 20, end_of_day: true },
    pvt: { enabled: true, weekly_reminder: true },
    eeg_fatigue: { enabled: true },
  });
  await loadConfig();
}
</script>

<div class="flex flex-col gap-8 px-1 py-2">
  <!-- Header -->
  <SectionHeader
    title={t("validation.title")}
    description={t("validation.intro")}
  />

  <!-- Disclaimer banner -->
  <div
    class="flex items-start gap-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm leading-relaxed text-amber-700 dark:text-amber-400"
    role="note"
  >
    <span class="mt-0.5 flex-shrink-0" aria-hidden="true">⚠️</span>
    <span>{t("validation.disclaimer")}</span>
  </div>

  {#if loadError}
    <div class="rounded-md border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm">
      Failed to load validation config: {loadError}
    </div>
  {/if}

  {#if config}
    <!-- ── Master gates ─────────────────────────────────────────────── -->
    <SettingsCard>
      <CardContent class="flex flex-col gap-5 p-6">
        <h3 class="text-base font-medium leading-tight">{t("validation.master.title")}</h3>

        <ToggleRow
          checked={config.respect_flow}
          label={t("validation.master.respectFlow")}
          description={t("validation.master.respectFlowDesc")}
          ontoggle={toggleRespectFlow}
        />

        <div class="grid grid-cols-2 gap-4">
          <label class="flex flex-col gap-1.5">
            <span class="text-sm font-medium">{t("validation.master.quietBefore")}</span>
            <input
              type="number"
              min="0"
              max="23"
              value={config.quiet_before_hour}
              onchange={setQuietBefore}
              class="w-full rounded-md border bg-background px-3 py-2 text-sm"
            />
          </label>
          <label class="flex flex-col gap-1.5">
            <span class="text-sm font-medium">{t("validation.master.quietAfter")}</span>
            <input
              type="number"
              min="0"
              max="23"
              value={config.quiet_after_hour}
              onchange={setQuietAfter}
              class="w-full rounded-md border bg-background px-3 py-2 text-sm"
            />
          </label>
        </div>
        <p class="text-xs leading-relaxed text-muted-foreground">{t("validation.master.quietDesc")}</p>
      </CardContent>
    </SettingsCard>

    <!-- ── KSS ─────────────────────────────────────────────────────── -->
    <SettingsCard>
      <CardContent class="flex flex-col gap-5 p-6">
        <div class="flex flex-col gap-1.5">
          <h3 class="text-base font-medium leading-tight">{t("validation.kss.title")}</h3>
          <p class="text-sm leading-relaxed text-muted-foreground">{t("validation.kss.desc")}</p>
        </div>

        <ToggleRow
          checked={config.kss.enabled}
          label={t("validation.kss.enabled")}
          ontoggle={toggleKss}
        />

        {#if config.kss.enabled}
          <div class="ml-1 flex flex-col gap-5 border-l-2 border-border pl-5">
            <div class="grid grid-cols-2 gap-4">
              <label class="flex flex-col gap-1.5">
                <span class="text-sm">{t("validation.kss.maxPerDay")}</span>
                <input
                  type="number"
                  min="1"
                  max="20"
                  value={config.kss.max_per_day}
                  onchange={bindNumber("kss", "max_per_day")}
                  class="w-full rounded-md border bg-background px-3 py-2 text-sm"
                />
              </label>
              <label class="flex flex-col gap-1.5">
                <span class="text-sm">{t("validation.kss.minInterval")}</span>
                <input
                  type="number"
                  min="5"
                  max="600"
                  value={config.kss.min_interval_min}
                  onchange={bindNumber("kss", "min_interval_min")}
                  class="w-full rounded-md border bg-background px-3 py-2 text-sm"
                />
              </label>
            </div>

            <ToggleRow
              checked={config.kss.trigger_break_coach}
              label={t("validation.kss.triggerBreakCoach")}
              ontoggle={toggleKssBreakCoach}
            />
            <ToggleRow
              checked={config.kss.trigger_random}
              label={t("validation.kss.triggerRandom")}
              description={t("validation.kss.triggerRandomDesc")}
              ontoggle={toggleKssRandom}
            />

            {#if config.kss.trigger_random}
              <label class="flex flex-col gap-1.5">
                <span class="text-sm">{t("validation.kss.randomWeight")}</span>
                <input
                  type="number"
                  min="0"
                  max="1"
                  step="0.05"
                  value={config.kss.random_weight}
                  onchange={bindNumber("kss", "random_weight")}
                  class="w-full rounded-md border bg-background px-3 py-2 text-sm"
                />
              </label>
            {/if}
          </div>
        {/if}
      </CardContent>
    </SettingsCard>

    <!-- ── NASA-TLX ────────────────────────────────────────────────── -->
    <SettingsCard>
      <CardContent class="flex flex-col gap-5 p-6">
        <div class="flex flex-col gap-1.5">
          <h3 class="text-base font-medium leading-tight">{t("validation.tlx.title")}</h3>
          <p class="text-sm leading-relaxed text-muted-foreground">{t("validation.tlx.desc")}</p>
        </div>

        <ToggleRow
          checked={config.tlx.enabled}
          label={t("validation.tlx.enabled")}
          ontoggle={toggleTlx}
        />

        {#if config.tlx.enabled}
          <div class="ml-1 flex flex-col gap-5 border-l-2 border-border pl-5">
            <div class="grid grid-cols-2 gap-4">
              <label class="flex flex-col gap-1.5">
                <span class="text-sm">{t("validation.tlx.maxPerDay")}</span>
                <input
                  type="number"
                  min="1"
                  max="10"
                  value={config.tlx.max_per_day}
                  onchange={bindNumber("tlx", "max_per_day")}
                  class="w-full rounded-md border bg-background px-3 py-2 text-sm"
                />
              </label>
              <label class="flex flex-col gap-1.5">
                <span class="text-sm">{t("validation.tlx.minTaskMin")}</span>
                <input
                  type="number"
                  min="5"
                  max="300"
                  value={config.tlx.min_task_min}
                  onchange={bindNumber("tlx", "min_task_min")}
                  class="w-full rounded-md border bg-background px-3 py-2 text-sm"
                />
              </label>
            </div>
            <ToggleRow
              checked={config.tlx.end_of_day}
              label={t("validation.tlx.endOfDay")}
              ontoggle={toggleTlxEndOfDay}
            />
            <div>
              <Button onclick={() => (tlxOpen = true)} variant="outline">
                {t("validation.tlx.form.title")}
              </Button>
            </div>
          </div>
        {/if}
      </CardContent>
    </SettingsCard>

    <!-- ── PVT ─────────────────────────────────────────────────────── -->
    <SettingsCard>
      <CardContent class="flex flex-col gap-5 p-6">
        <div class="flex flex-col gap-1.5">
          <h3 class="text-base font-medium leading-tight">{t("validation.pvt.title")}</h3>
          <p class="text-sm leading-relaxed text-muted-foreground">{t("validation.pvt.desc")}</p>
        </div>

        <ToggleRow
          checked={config.pvt.enabled}
          label={t("validation.pvt.enabled")}
          ontoggle={togglePvt}
        />

        {#if config.pvt.enabled}
          <div class="ml-1 border-l-2 border-border pl-5">
            <ToggleRow
              checked={config.pvt.weekly_reminder}
              label={t("validation.pvt.weeklyReminder")}
              ontoggle={togglePvtWeekly}
            />
          </div>
        {/if}

        <div>
          <Button onclick={() => (pvtOpen = true)} variant="outline">
            {t("validation.pvt.runNow")}
          </Button>
        </div>
      </CardContent>
    </SettingsCard>

    <!-- ── EEG fatigue index ───────────────────────────────────────── -->
    <SettingsCard>
      <CardContent class="flex flex-col gap-5 p-6">
        <div class="flex flex-col gap-1.5">
          <h3 class="text-base font-medium leading-tight">{t("validation.eeg.title")}</h3>
          <p class="text-sm leading-relaxed text-muted-foreground">{t("validation.eeg.desc")}</p>
        </div>

        <ToggleRow
          checked={config.eeg_fatigue.enabled}
          label={t("validation.eeg.enabled")}
          ontoggle={toggleEegFatigue}
        />

        {#if config.eeg_fatigue.enabled}
          <div class="ml-1 grid grid-cols-2 gap-4 border-l-2 border-border pl-5">
            <label class="flex flex-col gap-1.5">
              <span class="text-sm">{t("validation.eeg.windowSecs")}</span>
              <input
                type="number"
                min="5"
                max="300"
                value={config.eeg_fatigue.window_secs}
                onchange={bindNumber("eeg_fatigue", "window_secs")}
                class="w-full rounded-md border bg-background px-3 py-2 text-sm"
              />
            </label>
            <div class="flex flex-col gap-1.5">
              <span class="text-sm">{t("validation.eeg.current")}</span>
              <div class="rounded-md border bg-muted px-3 py-2 text-sm font-mono">
                {fatigueIndex !== null ? fatigueIndex.toFixed(2) : t("validation.eeg.noHeadset")}
              </div>
            </div>
          </div>
        {/if}
      </CardContent>
    </SettingsCard>

    <!-- ── Calibration Week ────────────────────────────────────────── -->
    <SettingsCard>
      <CardContent class="flex flex-col gap-4 p-6">
        <div class="flex flex-col gap-1.5">
          <h3 class="text-base font-medium leading-tight">{t("validation.calibrationWeek.title")}</h3>
          <p class="text-sm leading-relaxed text-muted-foreground">{t("validation.calibrationWeek.desc")}</p>
        </div>
        <div>
          <Button onclick={startCalibrationWeek} variant="default">
            {t("validation.calibrationWeek.start")}
          </Button>
        </div>
      </CardContent>
    </SettingsCard>

    <!-- ── Recent results ──────────────────────────────────────────── -->
    <SettingsCard>
      <CardContent class="flex flex-col gap-3 p-6">
        <h3 class="text-base font-medium leading-tight">{t("validation.results.title")}</h3>
        {#if recentKssCount > 0}
          <p class="text-sm text-muted-foreground">
            {t("validation.results.kssCount", { 0: recentKssCount })}
          </p>
        {:else}
          <p class="text-sm leading-relaxed text-muted-foreground">{t("validation.results.empty")}</p>
        {/if}
      </CardContent>
    </SettingsCard>

    <!-- Save status footer -->
    {#if saveStatus !== "idle"}
      <div class="px-1 text-xs text-muted-foreground">
        {#if saveStatus === "saving"}{t("validation.save.saving")}{/if}
        {#if saveStatus === "saved"}{t("validation.save.saved")}{/if}
        {#if saveStatus === "error"}{t("validation.save.failed", { 0: saveError })}{/if}
      </div>
    {/if}
  {/if}
</div>

{#if pvtOpen && config}
  <PvtPanel onclose={() => (pvtOpen = false)} />
{/if}

{#if tlxOpen && config}
  <TlxForm onclose={() => (tlxOpen = false)} taskKind="manual" />
{/if}
