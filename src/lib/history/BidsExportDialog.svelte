<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Export recordings in a time range as a BIDS-EEG dataset (daemon: skill-bids). -->

<script lang="ts">
import { onDestroy } from "svelte";
import { Button } from "$lib/components/ui/button";
import * as Dialog from "$lib/components/ui/dialog";
import { Spinner } from "$lib/components/ui/spinner";
import { ToggleSwitch } from "$lib/components/ui/toggle-switch";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { onDaemonEvent } from "$lib/daemon/ws";
import { fmtBytes } from "$lib/format";
import { t } from "$lib/i18n/index.svelte";

interface Props {
  open: boolean;
  /** Initial range, Unix seconds. */
  startUtc: number;
  endUtc: number;
  /** Bounds of the day being viewed, for the "This day" preset. */
  dayStartUtc?: number;
  dayEndUtc?: number;
  /** Set when opened from one session: limits the export to it. */
  csvPath?: string | null;
}

let { open = $bindable(), startUtc, endUtc, dayStartUtc, dayEndUtc, csvPath = null }: Props = $props();

interface PreviewItem {
  source: string;
  startUtc: number;
  endUtc: number | null;
  deviceName: string | null;
  channelNames: string[];
  sampleRate: number;
}
interface ExportSummary {
  outDir: string;
  recordings: { eegFile: string; durationS: number; nLabels: number; nGaps: number }[];
  skipped: { source: string; reason: string }[];
  validation: { severity: string; code: string; message: string; path: string | null }[];
  totalBytes: number;
}

const tzOffsetSecs = new Date().getTimezoneOffset() * -60;

let fromLocal = $state("");
let toLocal = $state("");
let onlySession = $state(true);
let subject = $state("01");
let task = $state("neuroskill");
let powerLine = $state<"" | "50" | "60">("");
let includeEvents = $state(true);
let includePpg = $state(true);
let includeImu = $state(true);

let preview = $state<PreviewItem[] | null>(null);
let previewing = $state(false);
let exporting = $state(false);
let progress = $state<{ done: number; total: number } | null>(null);
let result = $state<ExportSummary | null>(null);
let error = $state<string | null>(null);

/** Unix seconds → value for <input type="datetime-local"> in local time. */
function toInput(secs: number): string {
  const d = new Date(secs * 1000 - new Date(secs * 1000).getTimezoneOffset() * 60_000);
  return d.toISOString().slice(0, 19);
}
function fromInput(v: string): number | null {
  if (!v) return null;
  const ms = new Date(v).getTime();
  return Number.isFinite(ms) ? ms / 1000 : null;
}

// Reset the form each time the dialog opens.
$effect(() => {
  if (!open) return;
  fromLocal = toInput(startUtc);
  toLocal = toInput(endUtc);
  onlySession = !!csvPath;
  result = null;
  error = null;
  progress = null;
});

const rangeStart = $derived(fromInput(fromLocal));
const rangeEnd = $derived(fromInput(toLocal));
const rangeInvalid = $derived(rangeStart !== null && rangeEnd !== null && rangeEnd <= rangeStart);

function options() {
  return {
    startUtc: rangeStart,
    endUtc: rangeEnd,
    recordings: onlySession && csvPath ? [csvPath] : [],
    subject,
    task,
    powerLineHz: powerLine ? Number(powerLine) : null,
    tzOffsetSecs,
    includeEvents,
    includePpg,
    includeImu,
  };
}

// Debounced preview of what the range would include.
let previewTimer: ReturnType<typeof setTimeout> | null = null;
$effect(() => {
  if (!open || exporting) return;
  const opts = options();
  if (rangeInvalid) {
    preview = [];
    return;
  }
  if (previewTimer) clearTimeout(previewTimer);
  previewTimer = setTimeout(async () => {
    previewing = true;
    try {
      const r = await daemonInvoke<{ recordings: PreviewItem[] }>("preview_bids_export", opts);
      preview = r.recordings;
    } catch (e) {
      preview = null;
      error = String(e);
    } finally {
      previewing = false;
    }
  }, 250);
});

const previewSeconds = $derived(
  (preview ?? []).reduce((acc, p) => acc + Math.max(0, (p.endUtc ?? rangeEnd ?? p.startUtc) - p.startUtc), 0),
);

function fmtSpan(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.round((secs % 3600) / 60);
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

function setRange(start: number, end: number) {
  fromLocal = toInput(start);
  toLocal = toInput(end);
}

let unsubscribe: (() => void) | null = null;
onDestroy(() => {
  unsubscribe?.();
  if (previewTimer) clearTimeout(previewTimer);
});

async function runExport() {
  exporting = true;
  error = null;
  result = null;
  progress = null;
  unsubscribe = onDaemonEvent("bids-export-progress", (ev) => {
    const p = ev.payload as { done: number; total: number };
    progress = { done: p.done, total: p.total };
  });
  try {
    const r = await daemonInvoke<{ ok: boolean; summary?: ExportSummary; error?: string }>("export_bids", options());
    if (r.ok && r.summary) result = r.summary;
    else error = r.error ?? t("history.bids.failed");
  } catch (e) {
    error = String(e);
  } finally {
    unsubscribe?.();
    unsubscribe = null;
    exporting = false;
  }
}

async function reveal() {
  if (!result) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(`${result.outDir}/dataset_description.json`);
}

const warnings = $derived((result?.validation ?? []).filter((v) => v.severity !== "error"));
const errors = $derived((result?.validation ?? []).filter((v) => v.severity === "error"));
const inputClass = "w-full min-w-0 text-ui-md px-2 py-1 rounded-md border border-border bg-background text-foreground";
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-xl">
    <Dialog.Header>
      <Dialog.Title>{t("history.bids.title")}</Dialog.Title>
      <Dialog.Description>{t("history.bids.description")}</Dialog.Description>
    </Dialog.Header>

    {#if result}
      <div class="flex flex-col gap-3 text-ui-md">
        <p>
          {t("history.bids.done", {
            count: String(result.recordings.length),
            size: fmtBytes(result.totalBytes),
          })}
        </p>
        <p class="font-mono text-ui-sm text-muted-foreground break-all">{result.outDir}</p>
        {#if errors.length === 0 && warnings.length === 0}
          <p class="text-emerald-600 dark:text-emerald-400">{t("history.bids.valid")}</p>
        {:else}
          <details>
            <summary class="cursor-pointer">
              {t("history.bids.issues", { errors: String(errors.length), warnings: String(warnings.length) })}
            </summary>
            <ul class="mt-1 text-ui-sm text-muted-foreground list-disc ps-5">
              {#each result.validation as v}
                <li>{v.severity}: {v.message}{v.path ? ` — ${v.path}` : ""}</li>
              {/each}
            </ul>
          </details>
        {/if}
        {#if result.skipped.length > 0}
          <details>
            <summary class="cursor-pointer">{t("history.bids.skipped", { count: String(result.skipped.length) })}</summary>
            <ul class="mt-1 text-ui-sm text-muted-foreground list-disc ps-5">
              {#each result.skipped as s}
                <li class="break-all">{s.source.split("/").pop()}: {s.reason}</li>
              {/each}
            </ul>
          </details>
        {/if}
      </div>
      <Dialog.Footer>
        <Button variant="outline" onclick={() => { open = false; }}>{t("common.close")}</Button>
        <Button onclick={reveal}>{t("history.bids.reveal")}</Button>
      </Dialog.Footer>
    {:else}
      <div class="flex flex-col gap-3 text-ui-md">
        <div class="grid grid-cols-2 gap-2">
          <label class="flex flex-col gap-1">
            <span class="text-ui-sm text-muted-foreground">{t("history.bids.from")}</span>
            <input type="datetime-local" step="1" bind:value={fromLocal} class={inputClass} disabled={exporting} />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-ui-sm text-muted-foreground">{t("history.bids.to")}</span>
            <input type="datetime-local" step="1" bind:value={toLocal} class={inputClass} disabled={exporting} />
          </label>
        </div>
        <div class="flex flex-wrap gap-1.5">
          {#if csvPath}
            <Button size="sm" variant="outline" class="h-6 text-ui-sm" disabled={exporting}
                    onclick={() => setRange(startUtc, endUtc)}>{t("history.bids.presetSession")}</Button>
          {/if}
          {#if dayStartUtc !== undefined && dayEndUtc !== undefined}
            <Button size="sm" variant="outline" class="h-6 text-ui-sm" disabled={exporting}
                    onclick={() => setRange(dayStartUtc, dayEndUtc)}>{t("history.bids.presetDay")}</Button>
          {/if}
          <Button size="sm" variant="outline" class="h-6 text-ui-sm" disabled={exporting}
                  onclick={() => { const now = Date.now() / 1000; setRange(now - 7 * 86400, now); }}>
            {t("history.bids.presetWeek")}
          </Button>
          <Button size="sm" variant="outline" class="h-6 text-ui-sm" disabled={exporting}
                  onclick={() => { fromLocal = ""; toLocal = ""; }}>{t("history.bids.presetAll")}</Button>
        </div>
        {#if csvPath}
          <ToggleSwitch checked={onlySession} label={t("history.bids.onlySession")} disabled={exporting}
                        ontoggle={() => { onlySession = !onlySession; }} />
        {/if}

        <div class="grid grid-cols-3 gap-2">
          <label class="flex flex-col gap-1">
            <span class="text-ui-sm text-muted-foreground">{t("history.bids.subject")}</span>
            <input type="text" bind:value={subject} class={inputClass} disabled={exporting} />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-ui-sm text-muted-foreground">{t("history.bids.task")}</span>
            <input type="text" bind:value={task} class={inputClass} disabled={exporting} />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-ui-sm text-muted-foreground">{t("history.bids.powerLine")}</span>
            <select bind:value={powerLine} class={inputClass} disabled={exporting}>
              <option value="">{t("history.bids.powerLineUnknown")}</option>
              <option value="50">50 Hz</option>
              <option value="60">60 Hz</option>
            </select>
          </label>
        </div>

        <div class="flex flex-col gap-1.5">
          <ToggleSwitch checked={includeEvents} label={t("history.bids.includeEvents")} disabled={exporting}
                        ontoggle={() => { includeEvents = !includeEvents; }} />
          <ToggleSwitch checked={includePpg} label={t("history.bids.includePpg")} disabled={exporting}
                        ontoggle={() => { includePpg = !includePpg; }} />
          <ToggleSwitch checked={includeImu} label={t("history.bids.includeImu")} disabled={exporting}
                        ontoggle={() => { includeImu = !includeImu; }} />
        </div>

        <div class="text-ui-sm text-muted-foreground min-h-5" aria-live="polite">
          {#if rangeInvalid}
            <span class="text-red-500">{t("history.bids.rangeInvalid")}</span>
          {:else if exporting}
            <span class="flex items-center gap-2">
              <Spinner class="w-3 h-3" />
              {progress ? t("history.bids.progress", { done: String(progress.done), total: String(progress.total) }) : t("history.bids.exporting")}
            </span>
          {:else if previewing && preview === null}
            <Spinner class="w-3 h-3" />
          {:else if preview}
            {preview.length === 0
              ? t("history.bids.previewNone")
              : t("history.bids.preview", { count: String(preview.length), duration: fmtSpan(previewSeconds) })}
          {/if}
        </div>
        {#if error}
          <p class="text-ui-sm text-red-500 break-words">{error}</p>
        {/if}
      </div>
      <Dialog.Footer>
        <Button variant="outline" disabled={exporting} onclick={() => { open = false; }}>{t("common.cancel")}</Button>
        <Button disabled={exporting || rangeInvalid || !preview || preview.length === 0} onclick={runExport}>
          {t("history.bids.export")}
        </Button>
      </Dialog.Footer>
    {/if}
  </Dialog.Content>
</Dialog.Root>
