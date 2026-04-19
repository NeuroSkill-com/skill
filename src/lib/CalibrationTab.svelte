<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Calibration tab — multi-profile manager with N-action support. -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onDestroy, onMount } from "svelte";
import { Badge } from "$lib/components/ui/badge";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import {
  CALIBRATION_ACTION_DURATION_SECS,
  CALIBRATION_ACTION1_LABEL,
  CALIBRATION_ACTION2_LABEL,
  CALIBRATION_BREAK_DURATION_SECS,
  CALIBRATION_LOOP_COUNT,
} from "$lib/constants";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { daemonStatus } from "$lib/daemon/status.svelte";
import { fmtDateTimeLocale } from "$lib/format";
import { t } from "$lib/i18n/index.svelte";

// ── Types ──────────────────────────────────────────────────────────────────
interface CalibrationAction {
  label: string;
  duration_secs: number;
}
interface CalibrationProfile {
  id: string;
  name: string;
  actions: CalibrationAction[];
  break_duration_secs: number;
  loop_count: number;
  auto_start: boolean;
  last_calibration_utc: number | null;
}

// ── Quick presets ──────────────────────────────────────────────────────────
interface Preset {
  key: string;
  icon: string;
  actions: CalibrationAction[];
  breakSecs: number;
  loops: number;
}
const PRESETS: Preset[] = [
  {
    key: "baseline",
    icon: "👁",
    actions: [
      { label: "Eyes Open", duration_secs: 20 },
      { label: "Eyes Closed", duration_secs: 20 },
    ],
    breakSecs: 5,
    loops: 3,
  },
  {
    key: "focus",
    icon: "🧠",
    actions: [
      { label: "Mental Math", duration_secs: 30 },
      { label: "Deep Breathing", duration_secs: 30 },
    ],
    breakSecs: 10,
    loops: 3,
  },
  {
    key: "meditation",
    icon: "🧘",
    actions: [
      { label: "Active Thinking", duration_secs: 30 },
      { label: "Mindful Rest", duration_secs: 30 },
    ],
    breakSecs: 10,
    loops: 3,
  },
  {
    key: "sleep",
    icon: "🌙",
    actions: [
      { label: "Alert", duration_secs: 20 },
      { label: "Drowsy", duration_secs: 20 },
    ],
    breakSecs: 10,
    loops: 3,
  },
  {
    key: "gaming",
    icon: "🎮",
    actions: [
      { label: "Focus Task", duration_secs: 20 },
      { label: "Passive Rest", duration_secs: 20 },
    ],
    breakSecs: 10,
    loops: 3,
  },
  {
    key: "children",
    icon: "🧒",
    actions: [
      { label: "Active", duration_secs: 10 },
      { label: "Rest", duration_secs: 10 },
    ],
    breakSecs: 5,
    loops: 3,
  },
  {
    key: "clinical",
    icon: "🔬",
    actions: [
      { label: "Active", duration_secs: 30 },
      { label: "Rest", duration_secs: 30 },
    ],
    breakSecs: 15,
    loops: 5,
  },
  {
    key: "stress",
    icon: "💆",
    actions: [
      { label: "Calm Breathing", duration_secs: 20 },
      { label: "Stressor Task", duration_secs: 20 },
    ],
    breakSecs: 10,
    loops: 3,
  },
];

// ── State ──────────────────────────────────────────────────────────────────
let profiles = $state<CalibrationProfile[]>([]);
let activeId = $state<string>("");
let editing = $state<CalibrationProfile | null>(null);
let isNew = $state(false);
let saving = $state(false);
let now = $state(Math.floor(Date.now() / 1000));
let nowTimer: ReturnType<typeof setInterval>;

const activeProfile = $derived(profiles.find((p) => p.id === activeId) ?? profiles[0] ?? null);

// ── Helpers ────────────────────────────────────────────────────────────────
function timeAgo(utc: number): string {
  const diff = now - utc;
  if (diff < 60) return t("common.justNow");
  if (diff < 3600) return t("common.minutesAgo", { n: Math.floor(diff / 60) });
  if (diff < 86400) return t("common.hoursAgo", { n: Math.floor(diff / 3600) });
  return t("common.daysAgo", { n: Math.floor(diff / 86400) });
}

function fmtDate(utc: number) {
  return fmtDateTimeLocale(utc);
}

async function load() {
  profiles = await daemonInvoke<CalibrationProfile[]>("list_calibration_profiles");
  const active = await daemonInvoke<CalibrationProfile | null>("get_active_calibration");
  activeId = active?.id ?? profiles[0]?.id ?? "";
}

async function selectProfile(id: string) {
  activeId = id;
  await daemonInvoke("set_active_calibration", { id });
}

function startEditNew() {
  isNew = true;
  editing = {
    id: "",
    name: "",
    actions: [
      { label: CALIBRATION_ACTION1_LABEL, duration_secs: CALIBRATION_ACTION_DURATION_SECS },
      { label: CALIBRATION_ACTION2_LABEL, duration_secs: CALIBRATION_ACTION_DURATION_SECS },
    ],
    break_duration_secs: CALIBRATION_BREAK_DURATION_SECS,
    loop_count: CALIBRATION_LOOP_COUNT,
    auto_start: false,
    last_calibration_utc: null,
  };
}

function startEditExisting(p: CalibrationProfile) {
  isNew = false;
  editing = { ...p, actions: p.actions.map((a) => ({ ...a })) };
}

function cancelEdit() {
  editing = null;
}

async function saveEdit() {
  if (!editing) return;
  if (!editing.name.trim()) return;
  if (editing.actions.length === 0) return;
  saving = true;
  try {
    if (isNew) {
      const created = await daemonInvoke<CalibrationProfile>("create_calibration_profile", { profile: editing });
      await selectProfile(created.id);
    } else {
      await daemonInvoke("update_calibration_profile", { profile: editing });
    }
    editing = null;
    await load();
  } finally {
    saving = false;
  }
}

async function deleteProfile(id: string) {
  if (profiles.length <= 1) return;
  await daemonInvoke("delete_calibration_profile", { id });
  await load();
}

async function openCalibration() {
  await invoke("open_calibration_window");
}

function applyPreset(preset: Preset) {
  if (!editing) return;
  editing.actions = preset.actions.map((a) => ({ ...a }));
  editing.break_duration_secs = preset.breakSecs;
  editing.loop_count = preset.loops;
  if (!editing.name) editing.name = t(`calibration.preset.${preset.key}`);
}

function addAction() {
  if (!editing) return;
  editing.actions = [...editing.actions, { label: "", duration_secs: CALIBRATION_ACTION_DURATION_SECS }];
}
function removeAction(i: number) {
  if (!editing || editing.actions.length <= 1) return;
  editing.actions = editing.actions.filter((_, idx) => idx !== i);
}
function moveAction(i: number, dir: -1 | 1) {
  if (!editing) return;
  const j = i + dir;
  if (j < 0 || j >= editing.actions.length) return;
  const arr = [...editing.actions];
  [arr[i], arr[j]] = [arr[j], arr[i]];
  editing.actions = arr;
}

const totalSecs = $derived.by(() => {
  if (!editing) return 0;
  const actionTotal = editing.actions.reduce((s, a) => s + a.duration_secs, 0);
  const breakTotal = editing.loop_count * editing.actions.length * editing.break_duration_secs;
  return editing.loop_count * actionTotal + breakTotal;
});

onMount(async () => {
  await load();
  nowTimer = setInterval(() => {
    now = Math.floor(Date.now() / 1000);
  }, 10_000);
});
onDestroy(() => clearInterval(nowTimer));
</script>

<section class="flex flex-col gap-4">
  {#if daemonStatus.state !== 'connected'}
    <div class="rounded-lg border border-amber-500/20 bg-amber-500/5 p-3 flex items-center gap-2.5">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
           stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
           class="w-4 h-4 text-amber-600 dark:text-amber-400 shrink-0">
        <path d="M16 18v2a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h2"/>
        <path d="M12 4V2m0 2v4"/>
        <path d="m8 18 2-2 2 2"/>
        <path d="M12 12v.01"/>
        <path d="m16 14 1.5-1.5"/>
        <path d="M18.5 11.5L20 10"/>
      </svg>
      <span class="text-[0.68rem] text-amber-600 dark:text-amber-400 font-medium">
        {t("daemon.notConnectedWarning")}
      </span>
    </div>
  {/if}

  <!-- ── Profile list ───────────────────────────────────────────────────────── -->
  {#if !editing}

    <div class="flex items-center justify-between">
      <span class="text-[0.78rem] font-semibold text-foreground">{t("calibration.profiles")}</span>
      <Button size="sm" variant="outline" class="text-[0.65rem] h-7 px-2.5 gap-1"
              onclick={startEditNew}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
             stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
          <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
        </svg>
        {t("calibration.newProfile")}
      </Button>
    </div>

    <div class="flex flex-col gap-2">
      {#each profiles as p}
        {@const isActive = p.id === activeId}
        <div class="rounded-xl border transition-all
                    {isActive
                      ? 'border-blue-500/40 bg-blue-500/5 dark:bg-blue-500/10'
                      : 'border-border dark:border-white/[0.07] bg-white dark:bg-[#14141e]'}">
          <div class="flex items-center gap-3 px-3 py-2.5">
            <!-- Select radio -->
            <button onclick={() => selectProfile(p.id)}
                    aria-label={p.name}
                    class="w-4 h-4 rounded-full border-2 shrink-0 transition-colors
                           {isActive ? 'border-blue-500 bg-blue-500' : 'border-muted-foreground/30'}">
            </button>

            <!-- Name + meta -->
            <div class="flex flex-col gap-0.5 flex-1 min-w-0">
              <span class="text-[0.72rem] font-semibold truncate">{p.name}</span>
              <div class="flex items-center gap-1.5 flex-wrap">
                {#each p.actions as a}
                  <Badge variant="outline"
                    class="text-[0.5rem] py-0 px-1 bg-muted border-border/50 text-muted-foreground">
                    {a.label}
                  </Badge>
                {/each}
                <span class="text-[0.5rem] text-muted-foreground/50">
                  {t("settings.nLoops", { n: p.loop_count })} · {p.break_duration_secs}s {t("calibration.breakLabel")}
                </span>
              </div>
              {#if p.last_calibration_utc}
                <span class="text-[0.5rem] text-muted-foreground/40">
                  {t("calibration.lastAtAgo", { date: fmtDate(p.last_calibration_utc), ago: timeAgo(p.last_calibration_utc) })}
                </span>
              {:else}
                <span class="text-[0.5rem] text-amber-600/60 dark:text-amber-400/50 italic">
                  {t("calibration.neverCalibrated")}
                </span>
              {/if}
            </div>

            <!-- Edit / delete -->
            <div class="flex items-center gap-1 shrink-0">
              <button onclick={() => startEditExisting(p)}
                      aria-label={t("calibration.editProfile")}
                      class="p-1.5 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                     stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
                  <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                  <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
                </svg>
              </button>
              {#if profiles.length > 1}
                <button onclick={() => deleteProfile(p.id)}
                        aria-label={t("history.delete")}
                        class="p-1.5 rounded hover:bg-red-500/10 transition-colors text-muted-foreground hover:text-red-500">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                       stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
                    <polyline points="3 6 5 6 21 6"/>
                    <path d="M19 6l-1 14H6L5 6"/>
                    <path d="M10 11v6"/><path d="M14 11v6"/>
                    <path d="M9 6V4h6v2"/>
                  </svg>
                </button>
              {/if}
            </div>
          </div>
        </div>
      {/each}
    </div>

    <!-- ── Launch button ─────────────────────────────────────────────────────── -->
    <div class="flex items-center gap-3 pt-1">
      <Button size="sm" class="text-[0.7rem] h-9 px-5 gap-1.5" onclick={openCalibration}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
             stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
          <polygon points="5 3 19 12 5 21 5 3"/>
        </svg>
        {t("settings.openCalibration")}
      </Button>
      <span class="text-[0.56rem] text-muted-foreground/50">
        {activeProfile ? `"${activeProfile.name}"` : ""}
      </span>
    </div>

  {:else}
    <!-- ── Edit / Create form ──────────────────────────────────────────────── -->

    <div class="flex items-center gap-2">
      <button onclick={cancelEdit} aria-label={t("common.cancel")} class="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
             stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
          <polyline points="15 18 9 12 15 6"/>
        </svg>
      </button>
      <span class="text-[0.78rem] font-semibold">
        {isNew ? t("calibration.newProfile") : t("calibration.editProfile")}
      </span>
    </div>

    <!-- Quick presets -->
    <div class="flex flex-col gap-1.5">
      <span class="text-[0.6rem] font-semibold text-muted-foreground/60 uppercase tracking-wider">
        {t("calibration.presets")}
      </span>
      <div class="grid grid-cols-3 gap-1.5">
        {#each PRESETS as preset}
          <button onclick={() => applyPreset(preset)}
                  title={t(`calibration.preset.${preset.key}Desc`)}
                  class="flex flex-col items-start gap-0.5 rounded-xl border px-2.5 py-2
                         text-left transition-all border-border dark:border-white/[0.07]
                         bg-muted/20 hover:bg-muted/40">
            <span class="text-sm">{preset.icon}</span>
            <span class="text-[0.6rem] font-semibold leading-tight text-foreground/80">
              {t(`calibration.preset.${preset.key}`)}
            </span>
            <span class="text-[0.52rem] text-muted-foreground/60">
              {preset.actions[0]?.duration_secs}s · {preset.loops}×
            </span>
          </button>
        {/each}
      </div>
    </div>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

        <!-- Name -->
        <div class="flex flex-col gap-1.5 px-4 py-3">
          <label for="calibration-profile-name" class="text-[0.6rem] font-semibold text-muted-foreground uppercase tracking-wider">
            {t("calibration.profileName")}
          </label>
          <input id="calibration-profile-name" type="text" bind:value={editing.name}
                 placeholder={t("calibration.profileNamePlaceholder")}
                 class="rounded-lg border border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28]
                        px-2.5 py-1.5 text-[0.72rem] text-foreground
                 focus:outline-none focus:ring-1 focus:ring-ring/50" />
        </div>

        <!-- Actions -->
        <div class="flex flex-col gap-2.5 px-4 py-3.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.actionLabels")}</span>
            <button onclick={addAction}
                  aria-label="Add action"
                  class="flex items-center gap-1 text-[0.6rem] font-semibold text-primary
                    hover:text-primary/80 transition-colors">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                   stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
                <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
              </svg>
              {t("calibration.addAction")}
            </button>
          </div>

          {#each editing.actions as action, i}
            <div class="flex items-center gap-2">
              <!-- Up/down -->
              <div class="flex flex-col gap-0.5">
                <button onclick={() => moveAction(i, -1)} disabled={i === 0}
                        aria-label={t("calibration.moveUp")}
                        class="p-0.5 rounded text-muted-foreground/40 hover:text-foreground disabled:opacity-20">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                       stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5">
                    <polyline points="18 15 12 9 6 15"/>
                  </svg>
                </button>
                <button onclick={() => moveAction(i, 1)} disabled={i === editing.actions.length - 1}
                        aria-label={t("calibration.moveDown")}
                        class="p-0.5 rounded text-muted-foreground/40 hover:text-foreground disabled:opacity-20">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                       stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5">
                    <polyline points="6 9 12 15 18 9"/>
                  </svg>
                </button>
              </div>
              <!-- Label -->
              <input type="text" bind:value={action.label}
                     aria-label={t("calibration.actionLabel")}
                     placeholder={t("calibration.actionLabel")}
                     class="flex-1 rounded-lg border border-border dark:border-white/[0.08]
                            bg-muted dark:bg-[#1a1a28] px-2.5 py-1.5
                  text-[0.72rem] text-foreground focus:outline-none focus:ring-1 focus:ring-ring/50" />
              <!-- Duration -->
              <div class="flex items-center gap-1">
                {#each [5,10,15,20,30] as secs}
                  <button onclick={() => action.duration_secs = secs}
                          class="rounded px-1.5 py-0.5 text-[0.55rem] font-semibold border transition-all
                                 {action.duration_secs === secs
                                   ? 'border-primary/50 bg-primary/10 text-primary'
                                   : 'border-border text-muted-foreground hover:text-foreground'}">
                    {secs}s
                  </button>
                {/each}
              </div>
              <!-- Remove -->
              {#if editing.actions.length > 1}
                <button onclick={() => removeAction(i)}
                        aria-label={t("calibration.removeAction")}
                        class="p-1 text-muted-foreground/40 hover:text-red-500 transition-colors">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                       stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
                    <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
                  </svg>
                </button>
              {/if}
            </div>
          {/each}
        </div>

        <!-- Break + loops -->
        <div class="flex flex-col gap-2.5 px-4 py-3.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.timing")}</span>
          <div class="flex gap-4">
            <div class="flex flex-col gap-1 flex-1">
              <span class="text-[0.6rem] font-semibold text-muted-foreground uppercase tracking-wider">
                {t("settings.breakDurationSecs")}
              </span>
              <div class="flex items-center gap-1.5 flex-wrap">
                {#each [3,5,10,15,20] as secs}
                  <button onclick={() => editing!.break_duration_secs = secs}
                          class="rounded-lg border px-2 py-1 text-[0.62rem] font-semibold transition-all
                                 {editing.break_duration_secs === secs
                                   ? 'border-primary/50 bg-primary/10 text-primary'
                                   : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
                    {secs}s
                  </button>
                {/each}
              </div>
            </div>
            <div class="flex flex-col gap-1 flex-1">
              <span class="text-[0.6rem] font-semibold text-muted-foreground uppercase tracking-wider">
                {t("settings.iterations")}
              </span>
              <div class="flex items-center gap-1.5 flex-wrap">
                {#each [1,2,3,5,10] as n}
                  <button onclick={() => editing!.loop_count = n}
                          class="rounded-lg border px-2 py-1 text-[0.62rem] font-semibold transition-all
                                 {editing.loop_count === n
                                   ? 'border-primary/50 bg-primary/10 text-primary'
                                   : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
                    {n}×
                  </button>
                {/each}
              </div>
            </div>
          </div>
        </div>

        <!-- Auto-start -->
        <div class="flex items-center gap-3 px-4 py-3.5">
          <button role="switch" aria-checked={editing.auto_start}
                  onclick={() => editing!.auto_start = !editing!.auto_start}
                  class="flex items-center gap-3 text-left w-full">
            <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                        {editing.auto_start ? 'bg-emerald-500' : 'bg-muted dark:bg-white/[0.08]'}">
              <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                          {editing.auto_start ? 'translate-x-4' : 'translate-x-0.5'}"></div>
            </div>
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-[0.72rem] font-semibold text-foreground leading-tight">{t("settings.autoStartOnLaunch")}</span>
              <span class="text-[0.58rem] text-muted-foreground leading-tight">{t("settings.autoStartDesc")}</span>
            </div>
          </button>
        </div>

        <!-- Summary -->
        <div class="flex items-center gap-2 flex-wrap px-4 py-3 bg-slate-50 dark:bg-[#111118]">
          <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground shrink-0">
            {t("settings.summary")}
          </span>
          {#each editing.actions as action, i}
            {@const colors = ["bg-primary/10 text-primary border-primary/20",
                              "bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20",
                              "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
                              "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20",
                              "bg-rose-500/10 text-rose-600 dark:text-rose-400 border-rose-500/20"]}
            <Badge variant="outline" class="text-[0.56rem] py-0 px-1.5 {colors[i % colors.length]}">
              {action.label || "?"}
            </Badge>
          {/each}
          <Badge variant="outline"
            class="text-[0.56rem] py-0 px-1.5 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20">
            {t("settings.nLoops", { n: editing.loop_count })}
          </Badge>
          <span class="ml-auto text-[0.56rem] text-muted-foreground/60 shrink-0">
            {t("settings.totalSecs", { n: totalSecs })}
          </span>
        </div>

      </CardContent>
    </Card>

    <!-- Save / Cancel -->
    <div class="flex items-center gap-2">
      <Button onclick={saveEdit} disabled={saving || !editing.name.trim() || editing.actions.length === 0}
              class="text-[0.7rem] h-8 px-5">
        {saving ? t("common.saving") : t("common.save")}
      </Button>
      <Button variant="outline" onclick={cancelEdit} class="text-[0.7rem] h-8 px-4">
        {t("common.cancel")}
      </Button>
    </div>

  {/if}

</section>
