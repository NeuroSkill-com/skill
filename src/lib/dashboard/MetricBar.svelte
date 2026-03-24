<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Thin horizontal progress bar used in dashboard metric cards.
  Extracts the repeated "h-1 rounded-full bg-black/8 dark:bg-white/10" pattern.
-->
<script lang="ts">
interface Props {
  /** Fill percentage (0–100, clamped). */
  value: number;
  /** Tailwind bg class for the fill, e.g. "bg-emerald-500". Ignored when `gradient` is set. */
  bg?: string;
  /** CSS background value for gradient fills, e.g. "linear-gradient(90deg, #a, #b)". */
  gradient?: string;
  /** Tailwind height class. Defaults to "h-1". */
  height?: string;
}

let { value, bg = "", gradient = "", height = "h-1" }: Props = $props();
</script>

<div class="{height} rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
  <div
    class="{height} rounded-full transition-all duration-500 {bg}"
    style="width:{Math.min(100, Math.max(0, value))}%; {gradient ? `background:${gradient}` : ''}"
  ></div>
</div>
