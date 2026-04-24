<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Brain state card for the EEG dashboard — shows flow, fatigue, streak. -->
<script lang="ts">
import { onMount } from "svelte";
import { getFatigue, getFlow, getStreak, startBrainPolling } from "$lib/stores/brain.svelte";

onMount(() => startBrainPolling());
</script>

{#if true}
{@const flow = getFlow()}
{@const fatigue = getFatigue()}
{@const streak = getStreak()}
{#if flow || fatigue || streak}
  <div class="rounded-xl border border-border dark:border-white/[0.06] bg-surface-1 p-3 flex flex-col gap-2">
    <span class="text-ui-2xs font-semibold tracking-widest uppercase text-muted-foreground/50">Brain State</span>

    <div class="flex items-center gap-3">
      <!-- Flow indicator -->
      {#if flow?.in_flow}
        <div class="flex items-center gap-1.5">
          <span class="relative flex h-3 w-3">
            <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
            <span class="relative inline-flex rounded-full h-3 w-3 bg-emerald-500"></span>
          </span>
          <div>
            <div class="text-ui-sm font-bold text-emerald-500">FLOW</div>
            <div class="text-ui-2xs text-emerald-500/60 tabular-nums">{Math.floor(flow.duration_secs / 60)}m</div>
          </div>
        </div>
      {:else if flow}
        <div class="flex items-center gap-1.5">
          <span class="w-3 h-3 rounded-full bg-muted-foreground/20"></span>
          <div>
            <div class="text-ui-sm text-muted-foreground/50 tabular-nums">{flow.score.toFixed(0)}</div>
            <div class="text-ui-2xs text-muted-foreground/30">{flow.edit_velocity.toFixed(1)} ln/m</div>
          </div>
        </div>
      {/if}

      <!-- Fatigue -->
      {#if fatigue?.fatigued}
        <div class="flex items-center gap-1.5 px-2 py-1 rounded-md bg-amber-500/10 border border-amber-500/20">
          <span class="text-ui-sm font-bold text-amber-500">TIRED</span>
          <span class="text-ui-2xs text-amber-500/60">{fatigue.continuous_work_mins}m</span>
        </div>
      {/if}

      <!-- Streak -->
      {#if streak && streak.current_streak_days > 0}
        <div class="flex items-baseline gap-0.5 ml-auto">
          <span class="text-ui-lg font-bold text-violet-500 tabular-nums">{streak.current_streak_days}</span>
          <span class="text-ui-2xs text-violet-500/60">day streak</span>
        </div>
      {/if}
    </div>

    <!-- Focus bar -->
    {#if flow?.avg_focus != null}
      <div class="w-full h-1 rounded-full bg-muted/40 overflow-hidden">
        <div class="h-full rounded-full transition-all duration-500
          {flow.avg_focus >= 70 ? 'bg-emerald-500' : flow.avg_focus >= 40 ? 'bg-amber-400' : 'bg-red-400'}"
          style="width:{flow.avg_focus}%"></div>
      </div>
    {/if}
  </div>
{/if}
{/if}
