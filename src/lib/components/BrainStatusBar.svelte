<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Persistent brain status bar — shown at the bottom of the app layout. -->
<script lang="ts">
import { onDestroy, onMount } from "svelte";
import { getFatigue, getFlow, getStreak, startBrainPolling, stopBrainPolling } from "$lib/stores/brain.svelte";

onMount(() => startBrainPolling());
onDestroy(() => stopBrainPolling());

function fmtMins(secs: number): string {
  const m = Math.floor(secs / 60);
  return m < 60 ? `${m}m` : `${Math.floor(m / 60)}h${m % 60}m`;
}

const flow = $derived(getFlow());
const fatigue = $derived(getFatigue());
const streak = $derived(getStreak());
</script>

<div class="flex items-center gap-3 px-3 h-6 text-ui-2xs tabular-nums select-none shrink-0
            border-t border-border dark:border-white/[0.06]
            bg-muted/30 dark:bg-[#0d0d14]">

  <!-- Flow state -->
  {#if flow?.in_flow}
    <span class="flex items-center gap-1 text-emerald-500 font-semibold">
      <span class="relative flex h-1.5 w-1.5">
        <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
        <span class="relative inline-flex rounded-full h-1.5 w-1.5 bg-emerald-500"></span>
      </span>
      FLOW {fmtMins(flow.duration_secs)}
    </span>
  {:else if flow && flow.score > 0}
    <span class="text-muted-foreground/50">
      focus {flow.score.toFixed(0)}
    </span>
  {/if}

  <!-- Fatigue -->
  {#if fatigue?.fatigued}
    <span class="text-amber-500 font-semibold">
      TIRED {fatigue.continuous_work_mins}m
    </span>
  {/if}

  <!-- Streak -->
  {#if streak && streak.current_streak_days > 0}
    <span class="text-violet-500/70">
      {streak.current_streak_days}d streak
    </span>
  {/if}

  <!-- Edit velocity -->
  {#if flow && flow.edit_velocity > 0}
    <span class="text-muted-foreground/30">
      {flow.edit_velocity.toFixed(1)} ln/min
    </span>
  {/if}

  <!-- Focus score -->
  {#if flow?.avg_focus != null}
    <span class="text-muted-foreground/30 ml-auto">
      <span class="inline-block w-1.5 h-1.5 rounded-full mr-0.5
        {flow.avg_focus >= 70 ? 'bg-emerald-500' : flow.avg_focus >= 40 ? 'bg-amber-400' : 'bg-red-400'}"></span>
      {flow.avg_focus.toFixed(0)}
    </span>
  {/if}

  <!-- Deep work today -->
  {#if streak && streak.today_deep_mins > 0}
    <span class="text-muted-foreground/30">
      {streak.today_deep_mins}m deep
    </span>
  {/if}
</div>
