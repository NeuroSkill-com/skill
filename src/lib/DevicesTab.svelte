<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Devices tab — paired and discovered BCI devices. -->
<script lang="ts">
  import { onMount, onDestroy }       from "svelte";
  import { invoke }                   from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";

  import { colorForRssi }             from "$lib/theme";
  import { Badge }                    from "$lib/components/ui/badge";
  import { Button }                   from "$lib/components/ui/button";
  import { Card, CardContent }        from "$lib/components/ui/card";
  import { Separator }                from "$lib/components/ui/separator";
  import { t }                        from "$lib/i18n/index.svelte";

  // ── Types ──────────────────────────────────────────────────────────────────
  interface DiscoveredDevice {
    id:               string;
    name:             string;
    last_seen:        number;
    last_rssi:        number;
    is_paired:        boolean;
    is_preferred:     boolean;
    hardware_version?: string | null;
  }
  interface ConnectedInfo {
    device_id:     string | null;
    serial_number: string | null;
    mac_address:   string | null;
  }

  // ── State ──────────────────────────────────────────────────────────────────
  let devices      = $state<DiscoveredDevice[]>([]);
  let connected    = $state<ConnectedInfo>({ device_id: null, serial_number: null, mac_address: null });
  let now          = $state(Math.floor(Date.now() / 1000));
  let revealSN     = $state(false);
  let revealMAC    = $state(false);

  // ── Helpers ────────────────────────────────────────────────────────────────
  const fmtRssi = (r: number) => r === 0 ? "—" : `${r} dBm`;

  function redact(v: string) {
    const parts = v.split('-');
    return [...parts.slice(0, -1).map(p => '*'.repeat(p.length)), parts.at(-1)].join('-');
  }

  function fmtLastSeen(ts: number) {
    if (ts === 0) return "never";
    const d = now - ts;
    if (d < 5)    return "just now";
    if (d < 60)   return `${d}s ago`;
    if (d < 3600) return `${Math.floor(d / 60)}m ago`;
    return `${Math.floor(d / 3600)}h ago`;
  }

  // ── Device images ──────────────────────────────────────────────────────────
  function museImage(name: string, hw?: string | null): string | null {
    const n = name.toLowerCase();
    const isAthena = hw === "p50" || n.includes("muses");
    if (isAthena)                                                              return "/devices/muse-s-athena.jpg";
    if (n.includes("muse-s") || n.includes("muse s"))                         return "/devices/muse-s.jpg";
    if (n.includes("muse-2") || n.includes("muse2") || n.includes("muse 2")) return "/devices/muse-2.jpg";
    if (n.includes("muse"))                                                    return "/devices/muse-1.jpg";
    if (n.includes("mw75") || n.includes("neurable"))                         return "/devices/mw75.jpg";
    return null;
  }

  // ── Sorted device lists ────────────────────────────────────────────────────
  const pairedDevices     = $derived(devices.filter(d => d.is_paired));
  const discoveredDevices = $derived(devices.filter(d => !d.is_paired));
  const hasNewUnpaired    = $derived(discoveredDevices.some(d => d.last_rssi !== 0));

  // ── Device actions ─────────────────────────────────────────────────────────
  async function setPreferred(id: string) {
    const cur = devices.find(d => d.id === id);
    devices = await invoke<DiscoveredDevice[]>("set_preferred_device", { id: cur?.is_preferred ? "" : id });
  }
  async function forget(id: string) {
    await invoke("forget_device", { id });
    devices = devices.map(d => d.id === id ? { ...d, is_paired: false } : d);
  }
  async function pairDevice(id: string) {
    devices = await invoke<DiscoveredDevice[]>("pair_device", { id });
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────
  let unlisteners: UnlistenFn[] = [];
  let nowTimer: ReturnType<typeof setInterval>;

  onMount(async () => {
    devices = await invoke<DiscoveredDevice[]>("get_devices");
    nowTimer = setInterval(() => now = Math.floor(Date.now() / 1000), 1000);

    unlisteners.push(
      await listen<DiscoveredDevice[]>("devices-updated", ev => { devices = ev.payload; }),
      await listen<ConnectedInfo>("muse-status", ev => {
        connected = {
          device_id:     ev.payload.device_id     ?? null,
          serial_number: ev.payload.serial_number ?? null,
          mac_address:   ev.payload.mac_address   ?? null,
        };
      }),
    );
  });
  onDestroy(() => {
    unlisteners.forEach(u => u());
    clearInterval(nowTimer);
  });
</script>

<section class="flex flex-col gap-4">

  <!-- ── Hero ───────────────────────────────────────────────────────────────── -->
  <div class="rounded-2xl border border-border dark:border-white/[0.06]
              bg-gradient-to-r from-cyan-500/10 via-blue-500/10 to-indigo-500/10
              dark:from-cyan-500/15 dark:via-blue-500/15 dark:to-indigo-500/15
              px-5 py-4 flex items-center gap-4">
    <div class="flex items-center justify-center w-11 h-11 rounded-xl
                bg-gradient-to-br from-cyan-500 to-blue-500
                shadow-lg shadow-cyan-500/25 dark:shadow-cyan-500/40 shrink-0">
      <svg viewBox="0 0 24 24" fill="none" stroke="white"
           stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"
           class="w-5 h-5">
        <path d="M22 12h-4l-3 9L9 3l-3 9H2"/>
      </svg>
    </div>
    <div class="flex flex-col gap-0.5">
      <span class="text-[0.82rem] font-bold">{t("devices.title")}</span>
      <span class="text-[0.55rem] text-muted-foreground/70">
        {t("devices.subtitle")}
      </span>
    </div>
    <span class="flex-1"></span>
    <div class="flex flex-col items-end gap-0.5">
      <span class="text-lg font-extrabold tabular-nums tracking-tight
                   bg-gradient-to-r from-cyan-500 to-blue-500
                   bg-clip-text text-transparent">
        {pairedDevices.length}
      </span>
      <span class="text-[0.45rem] text-muted-foreground/50">
        {t("devices.pairedCount", { n: String(pairedDevices.length) })}
      </span>
    </div>
  </div>

  <!-- ── Paired Devices ─────────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("devices.pairedDevices")}
    </span>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      {#if pairedDevices.length === 0}
        <CardContent class="flex flex-col items-center gap-2 py-8 text-center">
          <span class="text-3xl">📡</span>
          <p class="text-[0.78rem] text-foreground/70">{t("devices.noPaired")}</p>
          <p class="text-[0.68rem] text-muted-foreground leading-relaxed max-w-[260px]">
            {t("devices.noPairedHint")}
          </p>
        </CardContent>
      {:else}
        {#each pairedDevices as dev, i (dev.id)}
          {#if i > 0}<Separator class="bg-border dark:bg-white/[0.04]" />{/if}
          {@render deviceRow(dev)}
        {/each}
      {/if}
    </Card>
  </div>

  <!-- ── Discovered Devices ──────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("devices.discoveredDevices")}
    </span>

    {#if hasNewUnpaired}
      <div class="flex items-start gap-2.5 rounded-xl
                  border border-amber-400/40 bg-amber-50/80 dark:bg-amber-950/25
                  px-3 py-2.5">
        <span class="text-[1rem] shrink-0 mt-0.5">📡</span>
        <div class="flex flex-col gap-0.5 flex-1 min-w-0">
          <span class="text-[0.72rem] font-semibold text-amber-800 dark:text-amber-300 leading-tight">
            {t("settings.newDeviceNotice")}
          </span>
          <span class="text-[0.64rem] text-amber-700/70 dark:text-amber-400/70 leading-relaxed">
            {t("settings.newDeviceNoticeHint")}
          </span>
        </div>
      </div>
    {/if}

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      {#if discoveredDevices.length === 0}
        <CardContent class="flex flex-col items-center gap-2 py-6 text-center">
          <span class="text-2xl">🔍</span>
          <p class="text-[0.72rem] text-muted-foreground/70">{t("devices.noDiscovered")}</p>
          <p class="text-[0.62rem] text-muted-foreground/50 leading-relaxed max-w-[260px]">
            {t("devices.noDiscoveredHint")}
          </p>
        </CardContent>
      {:else}
        {#each discoveredDevices as dev, i (dev.id)}
          {#if i > 0}<Separator class="bg-border dark:bg-white/[0.04]" />{/if}
          {@render deviceRow(dev)}
        {/each}
      {/if}
    </Card>
  </div>

</section>

<!-- ── Device row snippet ──────────────────────────────────────────────────── -->
{#snippet deviceRow(dev: DiscoveredDevice)}
  <div class="flex items-center gap-3 px-4 py-3
              transition-colors hover:bg-slate-50 dark:hover:bg-white/[0.02]
              {dev.is_preferred ? 'bg-blue-50 dark:bg-blue-950/20' : ''}
              {!dev.is_paired ? 'opacity-80' : ''}">

    <!-- Device photo -->
    {#if museImage(dev.name, dev.hardware_version)}
      <img src={museImage(dev.name, dev.hardware_version)!} alt={dev.name}
           class="w-12 h-12 object-contain rounded-lg shrink-0
                  bg-muted/40 dark:bg-white/[0.04] p-1
                  {!dev.is_paired ? 'grayscale opacity-60' : ''}" />
    {:else}
      <div class="w-12 h-12 rounded-lg shrink-0 bg-muted/40 dark:bg-white/[0.04]
                  flex items-center justify-center text-2xl
                  {!dev.is_paired ? 'opacity-50' : ''}">🧠</div>
    {/if}

    <div class="flex flex-col gap-0.5 min-w-0 flex-1">
      <div class="flex items-center gap-1.5 min-w-0">
        <span class="text-[0.82rem] font-semibold text-foreground truncate">{dev.name}</span>
        {#if dev.is_preferred}
          <span class="text-yellow-500 dark:text-yellow-400 shrink-0 text-sm">★</span>
        {/if}
        {#if dev.is_paired}
          <Badge variant="outline"
            class="text-[0.54rem] tracking-wide uppercase py-0 px-1 shrink-0
                   bg-green-500/10 text-green-600 dark:text-green-400 border-green-500/20">
            {t("settings.paired")}
          </Badge>
        {:else if dev.last_rssi !== 0}
          <Badge variant="outline"
            class="text-[0.54rem] tracking-wide uppercase py-0 px-1 shrink-0
                   bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20">
            {t("settings.new")}
          </Badge>
        {/if}
      </div>

      <div class="flex items-center gap-2">
        <span class="font-mono text-[0.6rem] text-muted-foreground truncate">{dev.id}</span>
        {#if dev.last_rssi !== 0}
          <span class="text-[0.6rem] shrink-0" style="color:{colorForRssi(dev.last_rssi)}">
            {fmtRssi(dev.last_rssi)}
          </span>
        {/if}
        <span class="text-[0.6rem] text-muted-foreground shrink-0">
          {fmtLastSeen(dev.last_seen)}
        </span>
      </div>

      {#if !dev.is_paired}
        <span class="text-[0.58rem] text-amber-600/80 dark:text-amber-400/70 leading-tight mt-0.5">
          {t("settings.pairToConnect")}
        </span>
      {/if}

      {#if dev.id === connected.device_id && (connected.serial_number || connected.mac_address)}
        <div class="flex items-center gap-3 flex-wrap">
          {#if connected.serial_number}
            <button
              onclick={() => revealSN = !revealSN}
              title={revealSN ? "Click to hide" : "Click to reveal"}
              class="font-mono text-[0.6rem] text-muted-foreground/80 hover:text-muted-foreground
                     cursor-pointer select-none transition-colors text-left">
              SN&nbsp;<span class="text-foreground/70">
                {revealSN ? connected.serial_number : redact(connected.serial_number)}
              </span>
            </button>
          {/if}
          {#if connected.mac_address}
            <button
              onclick={() => revealMAC = !revealMAC}
              title={revealMAC ? "Click to hide" : "Click to reveal"}
              class="font-mono text-[0.6rem] text-muted-foreground/80 hover:text-muted-foreground
                     cursor-pointer select-none transition-colors text-left">
              MAC&nbsp;<span class="text-foreground/70">
                {revealMAC ? connected.mac_address : redact(connected.mac_address)}
              </span>
            </button>
          {/if}
        </div>
      {/if}
    </div>

    <div class="flex items-center gap-1.5 shrink-0">
      {#if dev.is_paired}
        <Button
          size="sm"
          variant={dev.is_preferred ? "secondary" : "outline"}
          class={dev.is_preferred
            ? "text-[0.66rem] h-7 px-2.5 bg-yellow-500/15 text-yellow-600 dark:text-yellow-400 border-yellow-500/30 hover:bg-yellow-500/25"
            : "text-[0.66rem] h-7 px-2.5 border-border dark:border-white/10 text-muted-foreground hover:text-foreground"}
          onclick={() => setPreferred(dev.id)}>
          {dev.is_preferred ? t("settings.defaultDevice") : t("settings.setDefault")}
        </Button>
        <Button size="sm" variant="ghost"
          class="text-[0.66rem] h-7 px-2 text-muted-foreground hover:text-red-500"
          onclick={() => forget(dev.id)}>
          {t("settings.forget")}
        </Button>
      {:else}
        <Button
          size="sm"
          variant="outline"
          class="text-[0.66rem] h-7 px-2.5
                 border-amber-500/40 text-amber-700 dark:text-amber-400
                 hover:bg-amber-500/10 hover:border-amber-500/60"
          onclick={() => pairDevice(dev.id)}>
          {t("settings.pair")}
        </Button>
      {/if}
    </div>
  </div>
{/snippet}
