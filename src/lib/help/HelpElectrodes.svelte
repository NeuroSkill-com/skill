<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Help → Electrodes tab.
  Interactive 3D electrode guide with live signal quality from the device.
-->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import ElectrodeGuide         from "$lib/ElectrodeGuide.svelte";
  import { t }                  from "$lib/i18n/index.svelte";
  import type { DeviceStatus }    from "$lib/types";

  let quality = $state<string[]>(["no_signal","no_signal","no_signal","no_signal"]);
  let connected = $state(false);

  const unsubs: UnlistenFn[] = [];
  onMount(async () => {
    try {
      const s = await invoke<DeviceStatus>("get_status");
      quality   = s.channel_quality;
      connected = s.state === "connected";
    } catch (e) { console.warn("[help-electrodes] get_status failed:", e); }
    unsubs.push(
      await listen<DeviceStatus>("status", (ev) => {
        quality   = ev.payload.channel_quality;
        connected = ev.payload.state === "connected";
      })
    );
  });
  onDestroy(() => unsubs.forEach((u) => u()));
</script>

<div class="flex flex-col gap-3 py-2">
  {#if !connected}
    <div class="flex items-center gap-2 rounded-lg border border-amber-300/30 bg-amber-50 dark:bg-amber-950/20
                px-3 py-2 text-[0.65rem] text-amber-700 dark:text-amber-400">
      <span>⚠</span>
      <span>{t("electrode.notConnected")}</span>
    </div>
  {/if}

  <ElectrodeGuide qualityLabels={quality} />
</div>
