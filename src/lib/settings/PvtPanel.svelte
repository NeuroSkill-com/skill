<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!--
  Psychomotor Vigilance Task — modal panel.

  3-minute reaction-time task with random ITIs (2–10 s).  When the dot
  appears, user clicks (or presses any key) as fast as possible.  We
  measure RT with `performance.now()` for sub-millisecond resolution
  (paint-to-input is still bounded by the compositor frame; runs in a
  native Tauri window for that reason — VS Code webviews can't deliver
  this timing reliably).

  Output, posted to the daemon at `/v1/validation/pvt`:
    duration_secs, stimulus_count, response_count,
    mean_rt_ms, median_rt_ms, slowest10_rt_ms,
    lapse_count (RT > 500 ms), false_start_count (response without stimulus).
-->
<script lang="ts">
import { onDestroy, onMount } from "svelte";
import { Button } from "$lib/components/ui/button";
import { daemonPost } from "$lib/daemon/http";
import { t } from "$lib/i18n/index.svelte";
import { computeStats, LAPSE_THRESHOLD_MS } from "./pvt-stats";

let { onclose }: { onclose: () => void } = $props();

const TASK_DURATION_MS = 180_000; // 3 min
const ITI_MIN_MS = 2000;
const ITI_MAX_MS = 10_000;

type Phase = "intro" | "running" | "done" | "cancelled";

let phase = $state<Phase>("intro");
let elapsed = $state(0);
let stimulusVisible = $state(false);
let feedback = $state<"" | "go" | "tooFast">("");

// Recorded reaction times in milliseconds.
const rts: number[] = [];
let stimulusCount = 0;
let falseStartCount = 0;

let stimulusStartedAt = 0;
let startedAt = 0;
let scheduleHandle: ReturnType<typeof setTimeout> | undefined;
let elapsedHandle: ReturnType<typeof setInterval> | undefined;

function randomItiMs(): number {
  return ITI_MIN_MS + Math.random() * (ITI_MAX_MS - ITI_MIN_MS);
}

function start() {
  rts.length = 0;
  stimulusCount = 0;
  falseStartCount = 0;
  stimulusVisible = false;
  feedback = "";
  startedAt = performance.now();
  elapsed = 0;
  phase = "running";
  scheduleNext();
  elapsedHandle = setInterval(() => {
    elapsed = Math.min(TASK_DURATION_MS, performance.now() - startedAt);
    if (elapsed >= TASK_DURATION_MS) finish();
  }, 250);
}

function scheduleNext() {
  if (phase !== "running") return;
  const remaining = TASK_DURATION_MS - (performance.now() - startedAt);
  if (remaining < ITI_MIN_MS + 500) {
    return; // not enough time for one more trial — let timer finish
  }
  const iti = Math.min(randomItiMs(), remaining - 500);
  scheduleHandle = setTimeout(showStimulus, iti);
}

function showStimulus() {
  if (phase !== "running") return;
  stimulusVisible = true;
  stimulusStartedAt = performance.now();
  stimulusCount += 1;
  feedback = "go";
}

function onResponse() {
  if (phase !== "running") return;
  if (!stimulusVisible) {
    // False start — clicked / pressed before the dot appeared.
    falseStartCount += 1;
    feedback = "tooFast";
    return;
  }
  const rt = performance.now() - stimulusStartedAt;
  rts.push(rt);
  stimulusVisible = false;
  feedback = "";
  scheduleNext();
}

function cancel() {
  phase = "cancelled";
  if (scheduleHandle) clearTimeout(scheduleHandle);
  if (elapsedHandle) clearInterval(elapsedHandle);
  onclose();
}

function finish() {
  if (scheduleHandle) clearTimeout(scheduleHandle);
  if (elapsedHandle) clearInterval(elapsedHandle);
  phase = "done";
  void postResults();
}

async function postResults() {
  const stats = computeStats(rts);
  try {
    await daemonPost("/v1/validation/pvt", {
      duration_secs: Math.floor(TASK_DURATION_MS / 1000),
      stimulus_count: stimulusCount,
      response_count: stats.response_count,
      mean_rt_ms: stats.mean_rt_ms,
      median_rt_ms: stats.median_rt_ms,
      slowest10_rt_ms: stats.slowest10_rt_ms,
      lapse_count: stats.lapse_count,
      false_start_count: falseStartCount,
      fatigue_idx: null, // server fills from latest_bands when present
      started_at: Math.floor(Date.now() / 1000) - Math.floor(TASK_DURATION_MS / 1000),
      finished_at: Math.floor(Date.now() / 1000),
    });
  } catch (e) {}
}

function handleKey(e: KeyboardEvent) {
  // Space, Enter, or any printable key counts as a response.
  if (phase === "running") {
    e.preventDefault();
    onResponse();
  } else if (e.key === "Escape") {
    cancel();
  }
}

onMount(() => {
  window.addEventListener("keydown", handleKey);
});
onDestroy(() => {
  window.removeEventListener("keydown", handleKey);
  if (scheduleHandle) clearTimeout(scheduleHandle);
  if (elapsedHandle) clearInterval(elapsedHandle);
});

// ── Derived results (only meaningful in `done` phase) ──────────────────

let resultsSummary = $derived.by(() => {
  if (phase !== "done") return null;
  const s = computeStats(rts);
  return {
    meanRt: s.mean_rt_ms,
    medianRt: s.median_rt_ms,
    slowest10Rt: s.slowest10_rt_ms,
    lapses: s.lapse_count,
    falseStarts: falseStartCount,
    responses: s.response_count,
  };
});
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
  <div class="w-[480px] rounded-lg border bg-background p-8 shadow-xl">
    {#if phase === "intro"}
      <div class="flex flex-col gap-3">
        <h2 class="text-lg font-semibold leading-tight">{t("validation.pvt.task.title")}</h2>
        <p class="text-sm leading-relaxed text-muted-foreground">{t("validation.pvt.task.intro")}</p>
      </div>
      <div class="mt-8 flex justify-end gap-3">
        <Button variant="outline" onclick={cancel}>{t("validation.pvt.task.cancel")}</Button>
        <Button onclick={start}>{t("validation.pvt.task.start")}</Button>
      </div>
    {:else if phase === "running"}
      <div class="mb-4 text-xs text-muted-foreground">
        {t("validation.pvt.task.elapsed", {
          0: Math.floor(elapsed / 1000),
          1: Math.floor(TASK_DURATION_MS / 1000),
        })}
      </div>

      <button
        class="block h-72 w-full rounded-md border-2 border-dashed bg-muted transition-colors hover:bg-muted/80"
        onclick={onResponse}
        aria-label="PVT stimulus area"
      >
        {#if stimulusVisible}
          <div class="flex h-full items-center justify-center">
            <div class="h-12 w-12 rounded-full bg-emerald-500 shadow-lg shadow-emerald-500/50"></div>
          </div>
        {:else}
          <div class="flex h-full items-center justify-center text-sm text-muted-foreground">
            {feedback === "tooFast"
              ? t("validation.pvt.task.tooFast")
              : t("validation.pvt.task.wait")}
          </div>
        {/if}
      </button>

      <div class="mt-6 flex justify-end">
        <Button variant="outline" onclick={cancel}>{t("validation.pvt.task.cancel")}</Button>
      </div>
    {:else if phase === "done" && resultsSummary}
      <h2 class="text-lg font-semibold leading-tight">{t("validation.pvt.task.results")}</h2>
      <dl class="mt-5 grid grid-cols-2 gap-x-6 gap-y-3 text-sm">
        <dt class="text-muted-foreground">{t("validation.pvt.task.meanRt")}</dt>
        <dd class="font-mono tabular-nums">{resultsSummary.meanRt.toFixed(1)} ms</dd>

        <dt class="text-muted-foreground">{t("validation.pvt.task.medianRt")}</dt>
        <dd class="font-mono tabular-nums">{resultsSummary.medianRt.toFixed(1)} ms</dd>

        <dt class="text-muted-foreground">{t("validation.pvt.task.slowest10")}</dt>
        <dd class="font-mono tabular-nums">{resultsSummary.slowest10Rt.toFixed(1)} ms</dd>

        <dt class="text-muted-foreground">{t("validation.pvt.task.lapses")}</dt>
        <dd class="font-mono tabular-nums">{resultsSummary.lapses} / {resultsSummary.responses}</dd>

        <dt class="text-muted-foreground">{t("validation.pvt.task.falseStarts")}</dt>
        <dd class="font-mono tabular-nums">{resultsSummary.falseStarts}</dd>
      </dl>
      <div class="mt-8 flex justify-end">
        <Button onclick={onclose}>{t("validation.pvt.task.close")}</Button>
      </div>
    {/if}
  </div>
</div>
