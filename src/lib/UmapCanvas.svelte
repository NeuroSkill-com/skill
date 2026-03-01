<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Static wrapper that imports Threlte Canvas and UmapScene normally.
  This avoids svelte:component issues with Threlte's snippet children.
-->
<script lang="ts">
  import { Canvas } from "@threlte/core";
  import UmapScene from "./UmapScene.svelte";

  interface UmapPoint {
    x: number; y: number; z: number;
    session: number; utc: number; label?: string;
  }
  interface UmapResult {
    points: UmapPoint[]; n_a: number; n_b: number; dim: number;
  }

  let {
    data,
    tooltip     = $bindable(null),
    activeLabel = $bindable(null),
  }: {
    data: UmapResult;
    tooltip?: { x: number; y: number; text: string } | null;
    activeLabel?: string | null;
  } = $props();
</script>

<Canvas renderMode="always">
  <UmapScene {data} bind:tooltip bind:activeLabel />
</Canvas>
