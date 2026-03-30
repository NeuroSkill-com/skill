<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- Calendar heatmap views (year / month / week) for session history. -->
<script lang="ts">
import { fade } from "svelte/transition";
import { t } from "$lib/i18n/index.svelte";

export interface CalendarCell {
  dayKey: string;
  date: Date;
  count: number;
  inRange: boolean;
  isToday: boolean;
}

interface WeekGridDay {
  dayKey: string;
  label: string;
  sessions: Array<{ csv_path: string; start_utc: number; end_utc: number }>;
}

interface Props {
  viewMode: "year" | "month" | "week";
  calendarCells: CalendarCell[];
  yearWeeks: CalendarCell[][];
  maxCount: number;
  heatColor: (count: number, max: number) => string;
  navigateToDay: (dayKey: string) => void;
  recordingStreak: number;
  calendarMonth: string;
  weekGridDays: WeekGridDay[];
  // biome-ignore lint/suspicious/noExplicitAny: opaque session/grid data passed through from parent
  drawDayDots: (canvas: HTMLCanvasElement, sessions: any[], dayKey: string) => void;
  // biome-ignore lint/suspicious/noExplicitAny: opaque session/grid data passed through from parent
  renderDayDots: (canvas: HTMLCanvasElement, sessions: any[], dayKey: string) => void;
  // biome-ignore lint/suspicious/noExplicitAny: opaque session/grid data passed through from parent
  handleDayDotsHover: (canvas: HTMLCanvasElement, e: MouseEvent, sessions: any[], dayKey: string) => void;
  // biome-ignore lint/suspicious/noExplicitAny: opaque grid data passed through from parent
  drawDayGrid: (canvas: HTMLCanvasElement, data: any) => void;
  // biome-ignore lint/suspicious/noExplicitAny: opaque grid data passed through from parent
  renderDayGrid: (canvas: HTMLCanvasElement, data: any) => void;
  // biome-ignore lint/suspicious/noExplicitAny: opaque grid data passed through from parent
  handleGridHover: (canvas: HTMLCanvasElement, e: MouseEvent, data: any) => void;
  // biome-ignore lint/suspicious/noExplicitAny: opaque grid data returned from parent
  gridDataForDay: (dayKey: string) => any;
  // biome-ignore lint/suspicious/noExplicitAny: opaque session objects from parent
  daySessionsMap: Map<string, any[]>;
  gridTooltip: {
    visible: boolean;
    x: number;
    y: number;
    label: string;
    values: Array<{ label: string; value: string }>;
  } | null;
}

let {
  viewMode,
  calendarCells,
  yearWeeks,
  maxCount,
  heatColor,
  navigateToDay,
  recordingStreak,
  calendarMonth,
  weekGridDays,
  drawDayDots,
  renderDayDots,
  handleDayDotsHover,
  drawDayGrid,
  renderDayGrid,
  handleGridHover,
  gridDataForDay,
  daySessionsMap,
  gridTooltip,
}: Props = $props();
</script>

<div class="flex flex-col gap-2" transition:fade={{ duration: 120 }}>
  {#if viewMode === "year"}
    <!-- Year heatmap (GitHub-style) -->
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] p-4 overflow-x-auto">
      <div class="flex gap-[3px] min-w-max">
        {#each yearWeeks as week, wi}
          <div class="flex flex-col gap-[3px]">
            {#each week as cell}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="w-[11px] h-[11px] rounded-[2px] transition-colors
                       {cell.count > 0 ? heatColor(cell.count, maxCount) : 'bg-muted/40 dark:bg-white/[0.04]'}
                       {cell.count > 0 ? 'cursor-pointer hover:ring-1 hover:ring-foreground/30' : ''}"
                title={cell.dayKey ? `${cell.dayKey}: ${cell.count} session${cell.count !== 1 ? 's' : ''}` : ''}
                onclick={() => { if (cell.count > 0) navigateToDay(cell.dayKey); }}
              ></div>
            {/each}
          </div>
        {/each}
      </div>

      <!-- Month labels along the top -->
      <div class="flex mt-1.5 text-[0.4rem] text-muted-foreground/40 select-none">
        {#each ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"] as month, i}
          <span class="flex-1 text-center">{month}</span>
        {/each}
      </div>

      <!-- Legend -->
      <div class="flex items-center gap-1 mt-2 text-[0.42rem] text-muted-foreground/50">
        <span>{t("history.heatmap.less")}</span>
        {#each [0,1,2,3,4] as level}
          <div class="w-[10px] h-[10px] rounded-[2px]
                      {level === 0 ? 'bg-muted/40 dark:bg-white/[0.04]'
                       : heatColor(level, 4)}"></div>
        {/each}
        <span>{t("history.heatmap.more")}</span>
      </div>
    </div>

  {:else if viewMode === "month"}
    <!-- Month calendar grid -->
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] p-4">
      <div class="text-center text-[0.65rem] font-semibold text-foreground mb-2">
        {calendarMonth}
      </div>

      <!-- Weekday headers -->
      <div class="grid grid-cols-7 gap-1 mb-1">
        {#each ["S","M","T","W","T","F","S"] as wd}
          <div class="text-[0.42rem] text-center text-muted-foreground/50 font-medium uppercase">{wd}</div>
        {/each}
      </div>

      <!-- Day cells -->
      <div class="grid grid-cols-7 gap-1">
        {#each calendarCells as cell}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="aspect-square rounded-md flex flex-col items-center justify-center relative transition-colors
                   {cell.inRange
                     ? cell.count > 0
                       ? heatColor(cell.count, maxCount) + ' cursor-pointer hover:ring-1 hover:ring-foreground/30'
                       : 'bg-muted/20 dark:bg-white/[0.02] cursor-default'
                     : 'opacity-0 pointer-events-none'}
                   {cell.isToday ? 'ring-1 ring-foreground/30' : ''}"
            title={cell.inRange ? `${cell.dayKey}: ${cell.count}` : ''}
            onclick={() => { if (cell.count > 0) navigateToDay(cell.dayKey); }}
          >
            <span class="text-[0.5rem] tabular-nums leading-none
                         {cell.count > 0 ? 'font-semibold' : 'text-muted-foreground/40'}">
              {cell.date.getDate()}
            </span>
            <!-- Mini duration bar at the bottom of the cell -->
            {#if cell.count > 0}
              <div class="absolute bottom-[3px] left-1/2 -translate-x-1/2
                          w-3/4 h-[2px] rounded-full bg-foreground/20 dark:bg-white/20"></div>
            {/if}
          </div>
        {/each}
      </div>
    </div>

  {:else if viewMode === "week"}
    <!-- Week timeline grid -->
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] p-3 overflow-x-auto">
      <!-- Hour labels header -->
      <div class="grid gap-px mb-1" style="grid-template-columns: 72px repeat(48, minmax(0,1fr))">
        <div></div>
        {#each [0,3,6,9,12,15,18,21] as hr}
          <div class="text-[0.38rem] text-muted-foreground/40 text-center" style="grid-column: span 6">
            {hr.toString().padStart(2,"0")}:00
          </div>
        {/each}
      </div>

      <!-- Day rows -->
      {#each weekGridDays as wgd (wgd.dayKey)}
        <div class="grid gap-px items-center" style="grid-template-columns: 72px repeat(48, minmax(0,1fr))">
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="text-[0.48rem] font-semibold tabular-nums text-muted-foreground/60 pr-2 text-right truncate
                       {wgd.sessions.length > 0 ? 'cursor-pointer hover:text-foreground' : ''}"
               onclick={() => { if (wgd.sessions.length > 0) navigateToDay(wgd.dayKey); }}>
            {wgd.label}
          </div>
          <div class="col-span-48 h-5 relative">
            {#each wgd.sessions as sess}
              {@const dayStart = new Date(wgd.dayKey + 'T00:00:00').getTime()/1000}
              {@const sOff = Math.max(0, sess.start_utc - dayStart)}
              {@const dur  = Math.max(60, sess.end_utc - sess.start_utc)}
              {@const left = (sOff / 86400 * 100).toFixed(2)}
              {@const width = Math.max(0.4, dur / 86400 * 100).toFixed(2)}
              <button type="button"
                   class="absolute top-0.5 bottom-0.5 rounded-sm bg-violet-500/60 dark:bg-violet-400/50
                          hover:bg-violet-600 dark:hover:bg-violet-300/70 transition-colors cursor-pointer border-0 p-0"
                   style="left:{left}%; width:{width}%"
                   title="{new Date(sess.start_utc*1000).toLocaleTimeString()} – {new Date(sess.end_utc*1000).toLocaleTimeString()}"
                   onclick={() => navigateToDay(wgd.dayKey)}
              ></button>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Streak/legend footer -->
  <div class="flex items-center gap-3 text-[0.48rem] text-muted-foreground/50 px-1 flex-wrap">
    <span>{calendarCells.filter(c => c.count > 0).length} {t("history.heatmap.activeDays")}</span>
    {#if recordingStreak > 0}
      <span>🔥 {recordingStreak}-{t("history.heatmap.dayStreak")}</span>
    {/if}
  </div>
</div>
