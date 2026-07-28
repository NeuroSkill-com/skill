<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- Onboarding checklist widget for the dashboard. -->
<script lang="ts">
import { t } from "$lib/i18n/index.svelte";

interface Step {
  key: string;
  label: string;
  done: boolean;
}

interface Props {
  steps: Step[];
  onDismiss: () => void;
  onStepClick?: (key: string) => void;
}

let { steps, onDismiss, onStepClick }: Props = $props();

const doneCount = $derived(steps.filter((s) => s.done).length);
const progress = $derived(steps.length > 0 ? (doneCount / steps.length) * 100 : 0);
</script>

<div class="rounded-xl border border-amber-400/20 bg-amber-50/60 dark:bg-amber-950/20 px-3.5 py-3 flex flex-col gap-2">
  <div class="flex items-center gap-1.5">
    <span class="text-ui-md font-semibold text-amber-800 dark:text-amber-300">
      {t("dashboard.gettingStarted")}
    </span>
    <span class="ml-auto text-ui-sm text-muted-foreground/60">
      {doneCount}/{steps.length}
    </span>
  </div>
  <!-- Mini progress bar -->
  <div class="h-1 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
    <div class="h-full rounded-full bg-amber-400 transition-all duration-700"
         style="width:{progress}%">
    </div>
  </div>
  <div class="flex flex-col gap-1 mt-0.5">
    {#each steps as step (step.key)}
      {#if step.done || !onStepClick}
        <div class="flex items-center gap-2 py-0.5">
          <div class="w-3.5 h-3.5 rounded-full flex items-center justify-center shrink-0
                      {step.done ? 'bg-emerald-500' : 'border border-border dark:border-white/20 bg-transparent'}">
            {#if step.done}
              <svg viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="3"
                   stroke-linecap="round" stroke-linejoin="round" class="w-2 h-2">
                <polyline points="20 6 9 17 4 12"/>
              </svg>
            {/if}
          </div>
          <span class="text-ui-base {step.done ? 'text-muted-foreground/60 line-through' : 'text-foreground/80'}">
            {step.label}
          </span>
        </div>
      {:else}
        <button
          type="button"
          onclick={() => onStepClick(step.key)}
          class="flex items-center gap-2 py-0.5 rounded-md text-left
                 hover:bg-amber-500/10 transition-colors group w-full"
        >
          <div class="w-3.5 h-3.5 rounded-full flex items-center justify-center shrink-0
                      border border-border dark:border-white/20 bg-transparent
                      group-hover:border-amber-500/50">
          </div>
          <span class="text-ui-base text-foreground/80 group-hover:text-amber-800 dark:group-hover:text-amber-300
                       underline-offset-2 group-hover:underline flex-1">
            {step.label}
          </span>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
               stroke-linecap="round" stroke-linejoin="round"
               class="w-2.5 h-2.5 text-muted-foreground/40 group-hover:text-amber-600 shrink-0">
            <path d="M9 18l6-6-6-6"/>
          </svg>
        </button>
      {/if}
    {/each}
  </div>
  <button onclick={onDismiss}
          class="text-ui-xs text-muted-foreground/40 hover:text-muted-foreground/70 mt-0.5 self-end underline underline-offset-2">
    {t("common.dismiss")}
  </button>
</div>
