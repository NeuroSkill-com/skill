<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!--
  NASA-TLX (raw, un-weighted) workload form.

  Six 0–100 sub-scales per Hart & Staveland (1988) / Hart (2006).  We use
  the raw version — no pairwise weighting — which Hart 2006 defends as
  comparable to the weighted version while removing 60+ extra clicks.

  Submitted to /v1/validation/tlx.
-->
<script lang="ts">
import { Button } from "$lib/components/ui/button";
import { daemonPost } from "$lib/daemon/http";
import { t } from "$lib/i18n/index.svelte";

let {
  onclose,
  taskKind = "manual",
  taskDurationSecs = null,
  promptId = null,
}: {
  onclose: () => void;
  taskKind?: string;
  taskDurationSecs?: number | null;
  promptId?: number | null;
} = $props();

// Default to mid-scale; users move sliders away from there.
let mental = $state(50);
let physical = $state(50);
let temporal = $state(50);
let performance = $state(50);
let effort = $state(50);
let frustration = $state(50);

let submitting = $state(false);
let error = $state("");

interface Scale {
  key: string;
  labelKey: string;
  descKey: string;
  // Performance has inverted endpoints — failure → perfect rather than low → high.
  inverted: boolean;
  bind: () => number;
  set: (v: number) => void;
}

const scales: Scale[] = [
  {
    key: "mental",
    labelKey: "validation.tlx.mental",
    descKey: "validation.tlx.mentalDesc",
    inverted: false,
    bind: () => mental,
    set: (v: number) => (mental = v),
  },
  {
    key: "physical",
    labelKey: "validation.tlx.physical",
    descKey: "validation.tlx.physicalDesc",
    inverted: false,
    bind: () => physical,
    set: (v: number) => (physical = v),
  },
  {
    key: "temporal",
    labelKey: "validation.tlx.temporal",
    descKey: "validation.tlx.temporalDesc",
    inverted: false,
    bind: () => temporal,
    set: (v: number) => (temporal = v),
  },
  {
    key: "performance",
    labelKey: "validation.tlx.performance",
    descKey: "validation.tlx.performanceDesc",
    inverted: true,
    bind: () => performance,
    set: (v: number) => (performance = v),
  },
  {
    key: "effort",
    labelKey: "validation.tlx.effort",
    descKey: "validation.tlx.effortDesc",
    inverted: false,
    bind: () => effort,
    set: (v: number) => (effort = v),
  },
  {
    key: "frustration",
    labelKey: "validation.tlx.frustration",
    descKey: "validation.tlx.frustrationDesc",
    inverted: false,
    bind: () => frustration,
    set: (v: number) => (frustration = v),
  },
];

async function submit() {
  submitting = true;
  error = "";
  try {
    await daemonPost("/v1/validation/tlx", {
      mental: Math.round(mental),
      physical: Math.round(physical),
      temporal: Math.round(temporal),
      performance: Math.round(performance),
      effort: Math.round(effort),
      frustration: Math.round(frustration),
      task_kind: taskKind,
      task_duration_secs: taskDurationSecs,
      surface: "tauri",
      prompt_id: promptId,
    });
    onclose();
  } catch (e) {
    error = String(e);
  } finally {
    submitting = false;
  }
}
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center overflow-y-auto bg-black/60 backdrop-blur-sm">
  <div class="m-6 w-[560px] rounded-lg border bg-background p-8 shadow-xl">
    <div class="flex flex-col gap-2">
      <h2 class="text-lg font-semibold leading-tight">{t("validation.tlx.form.title")}</h2>
      <p class="text-sm leading-relaxed text-muted-foreground">{t("validation.tlx.form.subtitle")}</p>
    </div>

    <div class="mt-7 flex flex-col gap-7">
      {#each scales as scale (scale.key)}
        <div class="flex flex-col gap-2">
          <div class="flex items-baseline justify-between gap-3">
            <label for="tlx-{scale.key}" class="text-sm font-medium">
              {t(scale.labelKey)}
            </label>
            <span class="font-mono text-xs tabular-nums text-muted-foreground">
              {Math.round(scale.bind())}
            </span>
          </div>
          {#if scale.descKey}
            <p class="text-xs leading-relaxed text-muted-foreground">{t(scale.descKey)}</p>
          {/if}
          <input
            id="tlx-{scale.key}"
            type="range"
            min="0"
            max="100"
            step="1"
            value={scale.bind()}
            oninput={(e) => scale.set(Number((e.target as HTMLInputElement).value))}
            class="mt-1 w-full"
          />
          <div class="flex justify-between text-[10px] uppercase tracking-wide text-muted-foreground">
            <span>
              {scale.inverted ? t("validation.tlx.failure") : t("validation.tlx.low")}
            </span>
            <span>
              {scale.inverted ? t("validation.tlx.perfect") : t("validation.tlx.high")}
            </span>
          </div>
        </div>
      {/each}
    </div>

    {#if error}
      <div class="mt-5 rounded-md border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm">
        {t("validation.save.failed", { 0: error })}
      </div>
    {/if}

    <div class="mt-8 flex justify-end gap-3">
      <Button variant="outline" onclick={onclose}>{t("validation.pvt.task.cancel")}</Button>
      <Button onclick={submit} disabled={submitting}>
        {submitting ? t("validation.save.saving") : t("validation.save.saved")}
      </Button>
    </div>
  </div>
</div>
