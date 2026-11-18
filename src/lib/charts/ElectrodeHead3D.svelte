<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  The 3D head with electrodes on the scalp. Drawn by threers in WebAssembly
  (crates/skill-viz3d, `HeadView`), which also places the electrodes on the
  head model, draws the Muse band and the selection ring, and hit-tests.
-->
<script lang="ts">
import { onMount } from "svelte";
import { type Electrode, type ElectrodeSystem, getElectrodes, regionColors } from "#lib/data/electrodes";
import { getResolved } from "#lib/stores/theme.svelte";
import { log } from "#lib/utils/logger";
import { type HeadViewer, hexColor, loadViz3d, runView } from "#lib/viz3d";

interface Props {
  system: ElectrodeSystem;
  /** When provided, overrides the electrode list derived from `system`. */
  electrodesOverride?: Electrode[] | null;
  onSelect?: (electrode: Electrode | null) => void;
  selectedName?: string | null;
  /** Position offset for the electrode group [x, y, z] */
  electrodePosition?: [number, number, number];
  /** Scale for the electrode group [x, y, z] */
  electrodeScale?: [number, number, number];
}

let {
  system,
  electrodesOverride = null,
  selectedName = null,
  electrodePosition = [0, 0, 0],
  electrodeScale = [1, 1, 1],
}: Props = $props();

let dark = $derived(getResolved() === "dark");
let visible = $derived(electrodesOverride ?? getElectrodes(system));

let canvas: HTMLCanvasElement;
let viewer: HeadViewer | null = $state(null);
let failed = $state(false);

onMount(() => {
  let gone = false;
  let stop = () => {};
  let made: HeadViewer | null = null;
  (async () => {
    try {
      const viz = await loadViz3d();
      const v = await viz.HeadViewer.create(canvas, dark);
      if (gone) {
        v.free();
        return;
      }
      made = v;
      const glb = await fetch("/models/head.glb").then((r) => r.arrayBuffer());
      if (gone) return;
      v.loadModel(new Uint8Array(glb));
      stop = runView(canvas, v);
      viewer = v;
    } catch (e) {
      log.error("[ElectrodeHead3D] 3D view unavailable:", e);
      failed = true;
    }
  })();
  return () => {
    gone = true;
    stop();
    viewer = null;
    made?.free();
  };
});

// Electrodes and the selection together: a new list clears the selection.
$effect(() => {
  if (!viewer) return;
  const els = visible;
  viewer.setElectrodes(
    els.map((e) => e.name),
    new Float32Array(els.flatMap((e) => e.pos)),
    new Uint32Array(els.map((e) => hexColor(regionColors[e.region]))),
    new Uint8Array(els.map((e) => (e.muse ? 1 : 0))),
  );
  viewer.select(selectedName ?? undefined);
});

$effect(() => {
  viewer?.setTheme(dark);
});

$effect(() => {
  viewer?.setElectrodeTransform(...electrodePosition, ...electrodeScale);
});

/** The electrode under normalised device coordinates, if any. */
export function hitTest(ndcX: number, ndcY: number): Electrode | null {
  const i = viewer?.hitTest(ndcX, ndcY) ?? -1;
  return i >= 0 ? (visible[i] ?? null) : null;
}
</script>

<canvas bind:this={canvas} class="absolute inset-0 block h-full w-full touch-none"></canvas>
{#if failed}
  <div class="absolute inset-0 flex items-center justify-center text-muted-foreground text-ui-md">
    3D view unavailable
  </div>
{/if}
