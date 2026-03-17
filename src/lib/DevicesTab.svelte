<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Devices tab — paired/discovered devices, OpenBCI config, signal processing, EEG embedding. -->
<script lang="ts">
  import { onMount, onDestroy }       from "svelte";
  import { invoke }                   from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { DEFAULT_FILTER_CONFIG,
           EMBEDDING_EPOCH_SECS,
           EMBEDDING_OVERLAP_SECS }   from "$lib/constants";

  import { colorForRssi }             from "$lib/theme";
  import { Badge }                    from "$lib/components/ui/badge";
  import { Button }                   from "$lib/components/ui/button";
  import { Card, CardContent }        from "$lib/components/ui/card";
  import { Separator }                from "$lib/components/ui/separator";
  import { SUPPORTED_COMPANIES, type SupportedCompanyId } from "$lib/supported-devices";
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
  type PowerlineFreq = "Hz60" | "Hz50";
  interface FilterConfig {
    sample_rate:        number;
    low_pass_hz:        number | null;
    high_pass_hz:       number | null;
    notch:              PowerlineFreq | null;
    notch_bandwidth_hz: number;
  }
  interface ConnectedInfo {
    device_id:     string | null;
    serial_number: string | null;
    mac_address:   string | null;
  }

  // ── State ──────────────────────────────────────────────────────────────────
  let devices      = $state<DiscoveredDevice[]>([]);
  let connected    = $state<ConnectedInfo>({ device_id: null, serial_number: null, mac_address: null });
  let filter       = $state<FilterConfig>({ ...DEFAULT_FILTER_CONFIG });
  let filterSaving = $state(false);
  let overlapSecs  = $state(EMBEDDING_OVERLAP_SECS);
  let overlapSaving = $state(false);
  let now          = $state(Math.floor(Date.now() / 1000));
  let revealSN     = $state(false);
  let revealMAC    = $state(false);

  // ── OpenBCI config ──────────────────────────────────────────────────────────
  type OpenBciBoard =
    | "ganglion" | "ganglion_wifi"
    | "cyton"    | "cyton_wifi"
    | "cyton_daisy" | "cyton_daisy_wifi"
    | "galea";
  interface OpenBciConfig {
    board:            OpenBciBoard;
    scan_timeout_secs: number;
    serial_port:      string;
    wifi_shield_ip:   string;
    wifi_local_port:  number;
    galea_ip:         string;
    channel_labels:   string[];
  }
  interface DeviceApiConfig {
    emotiv_client_id: string;
    emotiv_client_secret: string;
    idun_api_token: string;
  }
  const OPENBCI_DEFAULT: OpenBciConfig = {
    board: "ganglion", scan_timeout_secs: 10,
    serial_port: "", wifi_shield_ip: "", wifi_local_port: 3000,
    galea_ip: "", channel_labels: [],
  };
  let openbci          = $state<OpenBciConfig>({ ...OPENBCI_DEFAULT });
  let openbciSaved     = $state(false);
  let openbciChanged   = $state(false);
  let openbciConnecting = $state(false);
  let openbciError     = $state("");
  let openbciExpanded  = $state(false);
  let deviceApi        = $state<DeviceApiConfig>({ emotiv_client_id: "", emotiv_client_secret: "", idun_api_token: "" });
  let emotivApiChanged = $state(false);
  let emotivApiSaved   = $state(false);
  let emotivApiError   = $state("");
  let idunApiChanged   = $state(false);
  let idunApiSaved     = $state(false);
  let idunApiError     = $state("");
  let emotivSecretVisible = $state(false);
  let idunTokenVisible = $state(false);
  let emotivApiExpanded = $state(false);
  let idunApiExpanded   = $state(false);
  let supportedCompanyExpanded = $state<SupportedCompanyId | null>(null);
  let serialPorts      = $state<string[]>([]);
  let portsLoading     = $state(false);

  async function loadSerialPorts() {
    portsLoading = true;
    try { serialPorts = await invoke<string[]>("list_serial_ports"); } catch { serialPorts = []; }
    portsLoading = false;
  }

  async function saveOpenbci() {
    await invoke("set_openbci_config", { config: openbci });
    openbciChanged = false;
    openbciSaved   = true;
    setTimeout(() => { openbciSaved = false; }, 2000);
  }

  async function connectOpenbci() {
    if (openbciChanged) await saveOpenbci();
    openbciConnecting = true;
    openbciError = "";
    try {
      await invoke("connect_openbci");
    } catch (e: unknown) {
      openbciError = e instanceof Error ? e.message : String(e);
    } finally {
      openbciConnecting = false;
    }
  }

  async function saveEmotivApi() {
    emotivApiError = "";
    try {
      await invoke("set_device_api_config", { config: deviceApi });
      emotivApiChanged = false;
      emotivApiSaved   = true;
      setTimeout(() => { emotivApiSaved = false; }, 2000);
    } catch (e: unknown) {
      emotivApiError = e instanceof Error ? e.message : String(e);
    }
  }

  async function saveIdunApi() {
    idunApiError = "";
    try {
      await invoke("set_device_api_config", { config: deviceApi });
      idunApiChanged = false;
      idunApiSaved   = true;
      setTimeout(() => { idunApiSaved = false; }, 2000);
    } catch (e: unknown) {
      idunApiError = e instanceof Error ? e.message : String(e);
    }
  }

  const isBle    = $derived(openbci.board === "ganglion");
  const isSerial = $derived(openbci.board === "cyton" || openbci.board === "cyton_daisy");
  const isWifi   = $derived(["ganglion_wifi","cyton_wifi","cyton_daisy_wifi"].includes(openbci.board));
  const isGalea  = $derived(openbci.board === "galea");

  function openbciChannelLabel(i: number): string {
    return openbci.channel_labels[i] ?? "";
  }
  function setChannelLabel(i: number, val: string) {
    const arr = [...openbci.channel_labels];
    while (arr.length <= i) arr.push("");
    arr[i] = val;
    openbci = { ...openbci, channel_labels: arr };
    openbciChanged = true;
  }

  const channelCount = $derived(
    (openbci.board === "cyton_daisy" || openbci.board === "cyton_daisy_wifi") ? 16 :
    (openbci.board === "cyton"       || openbci.board === "cyton_wifi")       ? 8  :
     openbci.board === "galea"                                                ? 24 : 4
  );

  // ── Channel label presets ─────────────────────────────────────────────────
  type PresetMap = Record<string, { label: string; names: string[] }>;

  const PRESETS_4CH: PresetMap = {
    default:   { label: "OpenBCI defaults (Ch1-4)",    names: ["Ch1","Ch2","Ch3","Ch4"] },
    frontal:   { label: "Frontal (Fp1, Fp2, F7, F8)",  names: ["Fp1","Fp2","F7","F8"] },
    motor:     { label: "Motor (C3, Cz, C4, Fz)",      names: ["C3","Cz","C4","Fz"] },
    occipital: { label: "Occipital (O1, Oz, O2, Pz)",  names: ["O1","Oz","O2","Pz"] },
  };
  const PRESETS_8CH: PresetMap = {
    default:   { label: "OpenBCI defaults (Fp1-O2)",              names: ["Fp1","Fp2","C3","C4","P3","P4","O1","O2"] },
    frontal:   { label: "Frontal (8ch)",                          names: ["Fp1","Fp2","F3","F4","F7","F8","Fz","AFz"] },
    motor:     { label: "Motor strip (FC5-FC6 montage)",          names: ["FC5","FC3","FC1","FC2","FC4","FC6","C3","C4"] },
    temporal:  { label: "Temporal (T7/T8 montage)",               names: ["F7","T7","P7","O1","F8","T8","P8","O2"] },
  };
  const PRESETS_16CH: PresetMap = {
    default:   { label: "Full 10-20 (16ch)",                      names: ["Fp1","Fp2","F3","F4","C3","C4","P3","P4","O1","O2","F7","F8","T7","T8","Fz","Pz"] },
    frontal:   { label: "Bilateral frontal (16ch)",               names: ["Fp1","Fp2","AF3","AF4","F3","F4","F7","F8","FC1","FC2","FC5","FC6","Fz","AFz","FT7","FT8"] },
    motor:     { label: "Full motor (16ch)",                      names: ["FC5","FC3","FC1","FC2","FC4","FC6","C5","C3","C1","C2","C4","C6","CP5","CP3","CP4","CP6"] },
  };
  const PRESETS_24CH: PresetMap = {
    default:   { label: "Galea defaults (EMG 0-7, EEG 8-17, AUX 18-21)", names: ["EMG1","EMG2","EMG3","EMG4","EMG5","EMG6","EMG7","EMG8","Fp1","Fp2","F3","F4","C3","C4","P3","P4","O1","O2","AUX1","AUX2","AUX3","AUX4","Rsv1","Rsv2"] },
    eeg_only:  { label: "EEG channels only (label all as 10-20)",         names: ["F7","F3","Fz","F4","F8","C3","Cz","C4","T7","T8","P7","P3","Pz","P4","P8","O1","Oz","O2","TP9","TP10","FT9","FT10","PO9","PO10"] },
  };

  const LABEL_PRESETS: Record<OpenBciBoard, PresetMap> = {
    ganglion:        PRESETS_4CH,
    ganglion_wifi:   PRESETS_4CH,
    cyton:           PRESETS_8CH,
    cyton_wifi:      PRESETS_8CH,
    cyton_daisy:     PRESETS_16CH,
    cyton_daisy_wifi: PRESETS_16CH,
    galea:           PRESETS_24CH,
  };

  const defaultChannelNames = $derived(
    Object.values(LABEL_PRESETS[openbci.board])[0]?.names ?? []
  );

  const activePreset = $derived((() => {
    const presets = LABEL_PRESETS[openbci.board];
    for (const [id, p] of Object.entries(presets)) {
      const matches = p.names.length === channelCount &&
        p.names.every((n, i) => (openbci.channel_labels[i] ?? "") === n);
      if (matches) return id;
    }
    const allBlank = openbci.channel_labels.slice(0, channelCount).every(l => !l);
    return allBlank ? "default" : "__custom__";
  })());

  function applyPreset(id: string) {
    if (id === "__custom__") return;
    const presets = LABEL_PRESETS[openbci.board];
    const p = presets[id];
    if (!p) {
      openbci = { ...openbci, channel_labels: Array(channelCount).fill("") };
    } else {
      openbci = { ...openbci, channel_labels: [...p.names] };
    }
    openbciChanged = true;
  }

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
    if (n.includes("muse-s") || n.includes("muse s"))                         return "/devices/muse-s-gen1.jpg";
    if (n.includes("muse-2") || n.includes("muse2") || n.includes("muse 2")) return "/devices/muse-gen2.jpg";
    if (n.includes("muse"))                                                    return "/devices/muse-gen1.jpg";
    if (n.includes("mw75") || n.includes("neurable"))                         return "/devices/muse-mw75.jpg";
    return null;
  }

  function deviceImage(name: string, hw?: string | null): string | null {
    const muse = museImage(name, hw);
    if (muse) return muse;

    const n = name.toLowerCase();
    if (n.includes("idun") || n.includes("guardian") || n.startsWith("ige")) {
      return "/devices/idun-guardian.png";
    }
    if (n.includes("insight")) {
      return "/devices/emotiv-insight.webp";
    }
    if (n.includes("flex")) {
      return "/devices/emotiv-flex-saline.webp";
    }
    if (n.includes("mn8")) {
      return "/devices/emotiv-mn8.webp";
    }
    if (n.includes("x-trodes") || n.includes("xtrodes") || n.includes("x trodes")) {
      return "/devices/emotiv-x-trodes.webp";
    }
    if (n.includes("epoc") || n.includes("emotiv")) {
      return "/devices/emotiv-epoc-x.webp";
    }
    if (n.includes("hermes") || n.includes("nucleus") || n.includes("re-ak") || n.includes("reak")) {
      return "/devices/re-ak-nucleus-hermes.png";
    }

    return null;
  }

  const OPENBCI_IMAGES: Record<string, string> = {
    ganglion:         "/devices/openbci-ganglion.jpg",
    ganglion_wifi:    "/devices/openbci-ganglion-wifi.jpg",
    cyton:            "/devices/openbci-cyton.png",
    cyton_wifi:       "/devices/openbci-cyton-wifi.jpg",
    cyton_daisy:      "/devices/openbci-cyton-daisy.jpg",
    cyton_daisy_wifi: "/devices/openbci-cyton-daisy-wifi.jpg",
    galea:            "/devices/openbci-galea.jpg",
  };

  // ── Unpaired device banner logic ───────────────────────────────────────────
  const newUnpairedDevices = $derived(
    devices.filter(d => !d.is_paired && d.last_rssi !== 0)
  );
  const hasNewUnpaired = $derived(newUnpairedDevices.length > 0);

  function expandSupportedCompany(id: SupportedCompanyId) {
    supportedCompanyExpanded = supportedCompanyExpanded === id ? null : id;
    if (id === "openbci") openbciExpanded = true;
    if (id === "emotiv") emotivApiExpanded = true;
    if (id === "idun") idunApiExpanded = true;
  }

  // ── Sorted device lists ────────────────────────────────────────────────────
  const pairedDevices     = $derived(devices.filter(d => d.is_paired));
  const discoveredDevices = $derived(devices.filter(d => !d.is_paired));

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

  // ── Filter ─────────────────────────────────────────────────────────────────
  async function applyFilter(patch: Partial<FilterConfig>) {
    filter = { ...filter, ...patch };
    filterSaving = true;
    try { await invoke("set_filter_config", { config: filter }); }
    finally { filterSaving = false; }
  }
  const setNotch    = (v: PowerlineFreq | null) => applyFilter({ notch: v });
  const setHighPass = (hz: number | null)        => applyFilter({ high_pass_hz: hz });
  const setLowPass  = (hz: number | null)        => applyFilter({ low_pass_hz: hz });

  // ── Overlap ────────────────────────────────────────────────────────────────
  const OVERLAP_PRESETS: [string, number][] = [
    ["0 s — none",    0],
    ["1.25 s — 25%",  1.25],
    ["2.5 s — 50%",   2.5],
    ["3.75 s — 75%",  3.75],
    ["4.5 s — 90%",   4.5],
  ];

  async function setOverlap(secs: number) {
    overlapSecs   = secs;
    overlapSaving = true;
    try { await invoke("set_embedding_overlap", { overlapSecs: secs }); }
    finally { overlapSaving = false; }
  }

  // ── GPU / memory stats ────────────────────────────────────────────────────
  interface GpuStats {
    render:            number;
    tiler:             number;
    overall:           number;
    isUnifiedMemory:   boolean;
    totalMemoryBytes:  number | null;
    freeMemoryBytes:   number | null;
  }
  let gpuStats = $state<GpuStats | null>(null);

  // ── Lifecycle ──────────────────────────────────────────────────────────────
  let unlisteners: UnlistenFn[] = [];
  let nowTimer: ReturnType<typeof setInterval>;

  onMount(async () => {
    devices     = await invoke<DiscoveredDevice[]>("get_devices");
    filter      = await invoke<FilterConfig>("get_filter_config");
    overlapSecs = await invoke<number>("get_embedding_overlap");
    gpuStats    = await invoke<GpuStats | null>("get_gpu_stats").catch(() => null);

    openbci = await invoke<OpenBciConfig>("get_openbci_config");
    deviceApi = await invoke<DeviceApiConfig>("get_device_api_config");
    await loadSerialPorts();

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

  <!-- ── Supported Devices ─────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("settings.supportedDevices.title")}
    </span>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col gap-3 p-4">
        {#each SUPPORTED_COMPANIES as company, i (company.id)}
          {#if i > 0}<Separator class="bg-border dark:bg-white/[0.04]" />{/if}

          <div class="flex flex-col gap-2.5">
            <button
              onclick={() => expandSupportedCompany(company.id)}
              class="flex items-center justify-between w-full"
              aria-expanded={supportedCompanyExpanded === company.id}
            >
                <span class="text-[0.76rem] font-semibold text-foreground">{t(company.nameKey)}</span>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                   class="w-3 h-3 text-muted-foreground/50 transition-transform duration-200
                          {supportedCompanyExpanded === company.id ? 'rotate-180' : ''}">
                <path d="M6 9l6 6 6-6"/>
              </svg>
            </button>

            <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-2.5">
              {#each company.devices as item (item.nameKey)}
                <button
                  onclick={() => expandSupportedCompany(company.id)}
                  class="flex flex-col items-stretch gap-2 rounded-lg border border-border/70
                         dark:border-white/[0.06] bg-background/60 px-2.5 py-2.5 hover:bg-muted/50
                         min-h-[126px]"
                  aria-label={`${t(company.nameKey)} ${t(item.nameKey)}`}
                >
                  <div class="w-full h-16 rounded-md overflow-hidden">
                    <img src={item.image} alt={t(item.nameKey)} class="w-full h-full object-cover" />
                  </div>
                  <span class="text-[0.62rem] text-center leading-tight text-foreground/85 min-h-[30px] flex items-center justify-center">{t(item.nameKey)}</span>
                </button>
              {/each}
            </div>

            {#if supportedCompanyExpanded === company.id}
              <div class="rounded-lg border border-border/70 dark:border-white/[0.06] bg-muted/40 px-3 py-2.5">
                <p class="text-[0.64rem] font-medium text-foreground/85 mb-1">{t("settings.supportedDevices.howToConnect")}</p>
                <div class="flex flex-col gap-1">
                  {#each company.instructionKeys as lineKey (lineKey)}
                    <p class="text-[0.62rem] text-muted-foreground leading-relaxed">• {t(lineKey)}</p>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </CardContent>
    </Card>
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

  <!-- ── OpenBCI ────────────────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <button
      onclick={() => openbciExpanded = !openbciExpanded}
      class="flex items-center justify-between w-full px-0.5 group"
      aria-expanded={openbciExpanded}
    >
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("settings.openbci")}
      </span>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
           stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
           class="w-3 h-3 text-muted-foreground/50 transition-transform duration-200
                  {openbciExpanded ? 'rotate-180' : ''}">
        <path d="M6 9l6 6 6-6"/>
      </svg>
    </button>

    {#if openbciExpanded}
    <p class="text-[0.68rem] text-muted-foreground/70 px-0.5 leading-relaxed">
      {t("settings.openbciDesc")}
    </p>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] py-0 gap-0 overflow-hidden">
      <CardContent class="flex flex-col gap-4 p-4">

        <!-- Board selector -->
        <div class="flex flex-col gap-1.5">
          <p class="text-[0.68rem] font-medium text-foreground/80">{t("settings.openbciBoard")}</p>
          <div class="grid grid-cols-2 gap-x-4 gap-y-1">
            {#snippet boardRadio(val: OpenBciBoard, key: string)}
              <label class="flex items-center gap-1.5 cursor-pointer select-none text-[0.7rem] leading-snug">
                <input type="radio" name="openbci-board" value={val}
                  checked={openbci.board === val}
                  onchange={() => { openbci = { ...openbci, board: val }; openbciChanged = true; }}
                  class="accent-violet-500 shrink-0" />
                <span class="truncate">{t(key)}</span>
              </label>
            {/snippet}
            {@render boardRadio("ganglion",         "settings.openbciBoardGanglion")}
            {@render boardRadio("ganglion_wifi",    "settings.openbciBoardGanglionWifi")}
            {@render boardRadio("cyton",            "settings.openbciBoardCyton")}
            {@render boardRadio("cyton_wifi",       "settings.openbciBoardCytonWifi")}
            {@render boardRadio("cyton_daisy",      "settings.openbciBoardCytonDaisy")}
            {@render boardRadio("cyton_daisy_wifi", "settings.openbciBoardCytonDaisyWifi")}
            {@render boardRadio("galea",            "settings.openbciBoardGalea")}
          </div>

          {#if OPENBCI_IMAGES[openbci.board]}
            <div class="mt-2 flex justify-center">
              <img
                src={OPENBCI_IMAGES[openbci.board]}
                alt={openbci.board}
                class="h-36 max-w-full object-cover rounded-xl
                       bg-muted/30 dark:bg-white/[0.03]
                       border border-border dark:border-white/[0.06]
                       transition-all duration-200" />
            </div>
          {/if}
        </div>

        <Separator class="bg-border dark:bg-white/[0.04]" />

        <!-- BLE scan timeout -->
        {#if isBle}
          <div class="flex items-center gap-3">
            <label for="openbci-scan-timeout" class="text-[0.68rem] font-medium text-foreground/80 shrink-0">
              {t("settings.openbciScanTimeout")}
            </label>
            <input id="openbci-scan-timeout"
              type="number" min="3" max="60" step="1"
              bind:value={openbci.scan_timeout_secs}
              oninput={() => { openbciChanged = true; }}
              class="w-16 text-[0.73rem] text-center px-2 py-1 rounded-md border border-border
                     bg-background text-foreground tabular-nums" />
            <span class="text-[0.64rem] text-muted-foreground">{t("settings.openbciScanTimeoutSuffix")}</span>
          </div>
          <Separator class="bg-border dark:bg-white/[0.04]" />
        {/if}

        <!-- Serial port -->
        {#if isSerial}
          <div class="flex flex-col gap-1.5">
            <p class="text-[0.68rem] font-medium text-foreground/80">{t("settings.openbciSerialPort")}</p>
            <div class="flex gap-2 items-center">
              {#if serialPorts.length > 0}
                <select bind:value={openbci.serial_port} onchange={() => { openbciChanged = true; }}
                  class="flex-1 min-w-0 text-[0.73rem] px-2 py-1 rounded-md border border-border bg-background text-foreground">
                  <option value="">{t("settings.openbciSerialPortPlaceholder")}</option>
                  {#each serialPorts as p}<option value={p}>{p}</option>{/each}
                </select>
              {:else}
                <input type="text" bind:value={openbci.serial_port} oninput={() => { openbciChanged = true; }}
                  placeholder={t("settings.openbciSerialPortPlaceholder")}
                  class="flex-1 min-w-0 text-[0.73rem] font-mono px-2 py-1 rounded-md border border-border bg-background text-foreground" />
              {/if}
              <Button size="sm" variant="outline"
                class="text-[0.64rem] h-7 px-2.5 shrink-0 border-border dark:border-white/10"
                onclick={loadSerialPorts} disabled={portsLoading}>
                {portsLoading ? "…" : t("settings.openbciRefreshPorts")}
              </Button>
            </div>
            <span class="text-[0.62rem] text-muted-foreground">{t("settings.openbciSerialPortHint")}</span>
          </div>
          <Separator class="bg-border dark:bg-white/[0.04]" />
        {/if}

        <!-- WiFi Shield -->
        {#if isWifi}
          <div class="flex flex-col gap-2">
            <div class="flex items-center gap-3">
              <img src="/devices/openbci-wifi-shield.png" alt="OpenBCI WiFi Shield"
                   class="h-16 w-16 object-cover rounded-lg shrink-0
                          bg-muted/30 dark:bg-white/[0.03]
                          border border-border dark:border-white/[0.06]" />
              <p class="text-[0.68rem] font-medium text-foreground/80">{t("settings.openbciWifiShieldIp")}</p>
            </div>
            <input type="text" bind:value={openbci.wifi_shield_ip} oninput={() => { openbciChanged = true; }}
              placeholder={t("settings.openbciWifiShieldIpPlaceholder")}
              class="text-[0.73rem] font-mono px-2 py-1 rounded-md border border-border bg-background text-foreground" />
            <div class="flex items-center gap-3 mt-1">
              <label for="openbci-local-port" class="text-[0.68rem] font-medium text-foreground/80 shrink-0">
                {t("settings.openbciWifiLocalPort")}
              </label>
              <input id="openbci-local-port"
                type="number" min="1024" max="65535" step="1"
                bind:value={openbci.wifi_local_port}
                oninput={() => { openbciChanged = true; }}
                class="w-20 text-[0.73rem] text-center px-2 py-1 rounded-md border border-border
                       bg-background text-foreground tabular-nums" />
            </div>
          </div>
          <Separator class="bg-border dark:bg-white/[0.04]" />
        {/if}

        <!-- Galea IP -->
        {#if isGalea}
          <div class="flex flex-col gap-1.5">
            <p class="text-[0.68rem] font-medium text-foreground/80">{t("settings.openbciGaleaIp")}</p>
            <input type="text" bind:value={openbci.galea_ip} oninput={() => { openbciChanged = true; }}
              placeholder={t("settings.openbciGaleaIpPlaceholder")}
              class="text-[0.73rem] font-mono px-2 py-1 rounded-md border border-border bg-background text-foreground" />
          </div>
          <Separator class="bg-border dark:bg-white/[0.04]" />
        {/if}

        <!-- Channel labels -->
        <div class="flex flex-col gap-2">
          <span class="text-[0.68rem] font-medium text-foreground/80">
            {t("settings.openbciChannelLabels")}
            <span class="text-muted-foreground font-normal"> ({channelCount})</span>
          </span>

          {#if channelCount > 4}
            <p class="text-[0.62rem] text-amber-600 dark:text-amber-400">
              {t("settings.openbciChannelsBeyond4", { n: channelCount })}
            </p>
          {/if}

          <div class="flex items-center gap-2">
            <select
              value={activePreset === "__custom__" ? "__custom__" : activePreset}
              onchange={(e) => applyPreset((e.currentTarget as HTMLSelectElement).value)}
              class="flex-1 min-w-0 text-[0.68rem] h-7 px-2 rounded border border-border
                     bg-background text-foreground/80 cursor-pointer truncate">
              {#if activePreset === "__custom__"}
                <option value="__custom__">{t("settings.openbciPresetNone")}</option>
              {/if}
              {#each Object.entries(LABEL_PRESETS[openbci.board]) as [id, p]}
                <option value={id}>{p.label}</option>
              {/each}
            </select>
            <button onclick={() => applyPreset("__none__")}
              class="shrink-0 text-[0.62rem] h-7 px-2.5 rounded border border-border
                     text-muted-foreground hover:text-red-500 hover:border-red-400/50
                     transition-colors bg-background whitespace-nowrap">
              {t("settings.openbciClearLabels")}
            </button>
          </div>

          <div class="grid grid-cols-4 gap-x-2 gap-y-2">
            {#each Array.from({ length: channelCount }, (_, i) => i) as i}
              <div class="flex flex-col gap-0.5 min-w-0">
                <span class="text-[0.58rem] text-muted-foreground tabular-nums text-center">{i + 1}</span>
                <input type="text"
                  value={openbciChannelLabel(i)}
                  oninput={(e) => setChannelLabel(i, (e.currentTarget as HTMLInputElement).value)}
                  placeholder={defaultChannelNames[i] ?? `Ch${i+1}`}
                  class="w-full min-w-0 text-[0.7rem] font-mono text-center px-1 py-0.5 rounded
                         border border-border bg-background text-foreground
                         placeholder:text-muted-foreground/35
                      focus:outline-none focus:ring-1 focus:ring-ring/50" />
              </div>
            {/each}
          </div>

          <span class="text-[0.62rem] text-muted-foreground">{t("settings.openbciChannelLabelsHint")}</span>
        </div>

      </CardContent>
    </Card>

    <!-- Save + Connect -->
    <div class="flex items-center justify-end gap-2 px-0.5">
      <Button size="sm"
        variant={openbciSaved ? "secondary" : "outline"}
        class="text-[0.66rem] h-7 px-3
               {openbciSaved ? 'text-green-600 dark:text-green-400 border-green-500/30' :
                openbciChanged ? 'border-primary/50 text-primary' :
                'border-border dark:border-white/10 text-muted-foreground'}"
        onclick={saveOpenbci}
        disabled={!openbciChanged && !openbciSaved}>
        {openbciSaved ? t("settings.openbciSaved") : t("settings.openbciSave")}
      </Button>

      {#if !isBle}
        <Button size="sm" variant="default"
          class="text-[0.66rem] h-7 px-3 bg-primary hover:bg-primary/90 text-primary-foreground"
          onclick={connectOpenbci}
          disabled={openbciConnecting}>
          {openbciConnecting ? t("settings.openbciConnecting") : t("settings.openbciConnect")}
        </Button>
      {/if}
    </div>

    {#if openbciError}
      <p class="text-[0.65rem] text-red-500 px-0.5 -mt-1">{openbciError}</p>
    {/if}
    {/if}
  </div>

  <!-- ── Device API ────────────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <div class="flex items-center gap-2 px-0.5">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("settings.deviceApi.title")}
      </span>
    </div>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col gap-3 p-4">
        <button
          onclick={() => emotivApiExpanded = !emotivApiExpanded}
          class="flex items-center justify-between w-full px-0.5 group"
          aria-expanded={emotivApiExpanded}
        >
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.deviceApi.emotivTitle")}</span>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
               stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
               class="w-3 h-3 text-muted-foreground/50 transition-transform duration-200
                      {emotivApiExpanded ? 'rotate-180' : ''}">
            <path d="M6 9l6 6 6-6"/>
          </svg>
        </button>

        {#if emotivApiExpanded}
          <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
            {t("settings.deviceApi.emotivDesc")}
          </p>

          <div class="flex flex-col gap-1.5">
            <label for="emotiv-client-id" class="text-[0.68rem] font-medium text-foreground/80">{t("settings.deviceApi.clientId")}</label>
            <input
              id="emotiv-client-id"
              type="text"
              bind:value={deviceApi.emotiv_client_id}
              oninput={() => { emotivApiChanged = true; }}
              placeholder="Emotiv Cortex Client ID"
              class="text-[0.73rem] px-2 py-1 rounded-md border border-border bg-background text-foreground" />
          </div>

          <div class="flex flex-col gap-1.5">
            <label for="emotiv-client-secret" class="text-[0.68rem] font-medium text-foreground/80">{t("settings.deviceApi.clientSecret")}</label>
            <div class="flex items-center gap-2">
              <input
                id="emotiv-client-secret"
                type={emotivSecretVisible ? "text" : "password"}
                bind:value={deviceApi.emotiv_client_secret}
                oninput={() => { emotivApiChanged = true; }}
                placeholder="Emotiv Cortex Client Secret"
                class="flex-1 min-w-0 text-[0.73rem] px-2 py-1 rounded-md border border-border bg-background text-foreground" />
              <Button size="sm" variant="outline"
                class="text-[0.64rem] h-7 px-2.5 shrink-0 border-border dark:border-white/10"
                onclick={() => emotivSecretVisible = !emotivSecretVisible}>
                {emotivSecretVisible ? t("settings.deviceApi.hide") : t("settings.deviceApi.show")}
              </Button>
            </div>
          </div>

          <div class="flex justify-end">
            <Button size="sm"
              variant={emotivApiSaved ? "secondary" : "outline"}
              class="text-[0.66rem] h-7 px-3
                {emotivApiSaved ? 'text-green-600 dark:text-green-400 border-green-500/30' :
                emotivApiChanged ? 'border-primary/50 text-primary' :
                'border-border dark:border-white/10 text-muted-foreground'}"
              onclick={saveEmotivApi}
              disabled={!emotivApiChanged && !emotivApiSaved}>
              {emotivApiSaved ? t("settings.deviceApi.saved") : t("settings.deviceApi.save")}
            </Button>
          </div>
          {#if emotivApiError}
            <p class="text-[0.62rem] text-destructive">{emotivApiError}</p>
          {/if}
        {/if}

        <Separator class="bg-border dark:bg-white/[0.04]" />

        <button
          onclick={() => idunApiExpanded = !idunApiExpanded}
          class="flex items-center justify-between w-full px-0.5 group"
          aria-expanded={idunApiExpanded}
        >
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.deviceApi.idunTitle")}</span>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
               stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
               class="w-3 h-3 text-muted-foreground/50 transition-transform duration-200
                      {idunApiExpanded ? 'rotate-180' : ''}">
            <path d="M6 9l6 6 6-6"/>
          </svg>
        </button>

        {#if idunApiExpanded}
          <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
            {t("settings.deviceApi.idunDesc")}
          </p>
          <a
            href="https://idun.tech/"
            target="_blank"
            rel="noopener noreferrer"
            class="text-[0.62rem] text-primary hover:underline w-fit">
            {t("settings.deviceApi.idunDashboard")}
          </a>

          <div class="flex flex-col gap-1.5">
            <label for="idun-api-token" class="text-[0.68rem] font-medium text-foreground/80">{t("settings.deviceApi.apiToken")}</label>
            <div class="flex items-center gap-2">
              <input
                id="idun-api-token"
                type={idunTokenVisible ? "text" : "password"}
                bind:value={deviceApi.idun_api_token}
                oninput={() => { idunApiChanged = true; }}
                placeholder="IDUN API Token"
                class="flex-1 min-w-0 text-[0.73rem] px-2 py-1 rounded-md border border-border bg-background text-foreground" />
              <Button size="sm" variant="outline"
                class="text-[0.64rem] h-7 px-2.5 shrink-0 border-border dark:border-white/10"
                onclick={() => idunTokenVisible = !idunTokenVisible}>
                {idunTokenVisible ? t("settings.deviceApi.hide") : t("settings.deviceApi.show")}
              </Button>
            </div>
          </div>

          <div class="flex justify-end">
            <Button size="sm"
              variant={idunApiSaved ? "secondary" : "outline"}
              class="text-[0.66rem] h-7 px-3
                {idunApiSaved ? 'text-green-600 dark:text-green-400 border-green-500/30' :
                idunApiChanged ? 'border-primary/50 text-primary' :
                'border-border dark:border-white/10 text-muted-foreground'}"
              onclick={saveIdunApi}
              disabled={!idunApiChanged && !idunApiSaved}>
              {idunApiSaved ? t("settings.deviceApi.saved") : t("settings.deviceApi.save")}
            </Button>
          </div>
          {#if idunApiError}
            <p class="text-[0.62rem] text-destructive">{idunApiError}</p>
          {/if}
        {/if}
      </CardContent>
    </Card>
  </div>

  <!-- ── Signal Processing ──────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <div class="flex items-center gap-2 px-0.5">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("settings.signalProcessing")}
      </span>
      {#if filterSaving}
        <span class="text-[0.56rem] text-muted-foreground">{t("common.saving")}</span>
      {/if}
    </div>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

        <!-- Powerline notch -->
        <div class="flex flex-col gap-2.5 px-4 py-4">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.powerlineNotch")}</span>
          <div class="flex gap-2">
            {#each ([["Hz60","🇺🇸",t("settings.us60Hz"),t("settings.us60HzSub")],["Hz50","🇪🇺",t("settings.eu50Hz"),t("settings.eu50HzSub")]] as const) as [val, flag, label, sub]}
              <button onclick={() => setNotch(val)}
                class="flex flex-col items-center gap-1 rounded-xl border px-3 py-2.5 flex-1
                       transition-all cursor-pointer select-none
                       {filter.notch === val
                         ? 'border-primary/50 bg-primary/10'
                         : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
                <span class="text-[1rem]">{flag}</span>
                <span class="text-[0.7rem] font-semibold leading-tight
                             {filter.notch === val ? 'text-primary' : 'text-foreground'}">
                  {label}
                </span>
                <span class="text-[0.58rem] text-muted-foreground">{sub}</span>
                {#if filter.notch === val}
                  <span class="text-[0.52rem] font-bold tracking-widest uppercase text-primary mt-0.5">{t("common.active")}</span>
                {/if}
              </button>
            {/each}

            <button onclick={() => setNotch(null)}
              class="flex flex-col items-center gap-1 rounded-xl border px-3 py-2.5 flex-1
                     transition-all cursor-pointer select-none
                     {filter.notch === null
                       ? 'border-slate-400/40 bg-slate-100 dark:bg-white/[0.05]'
                       : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
              <span class="text-[1rem]">🔕</span>
              <span class="text-[0.7rem] font-semibold text-muted-foreground leading-tight">{t("common.off")}</span>
              <span class="text-[0.58rem] text-muted-foreground">{t("settings.noNotch")}</span>
              {#if filter.notch === null}
                <span class="text-[0.52rem] font-bold tracking-widest uppercase text-slate-500 mt-0.5">{t("common.active")}</span>
              {/if}
            </button>
          </div>
        </div>

        <!-- High-pass -->
        <div class="flex flex-col gap-2 px-4 py-3.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.highPassCutoff")}</span>
          <div class="flex items-center gap-1.5 flex-wrap">
            {#each ([null, 0.5, 1, 4, 8] as const) as hz}
              <button onclick={() => setHighPass(hz)}
                class="rounded-lg border px-3 py-1.5 text-[0.68rem] font-semibold
                       transition-all cursor-pointer select-none
                       {filter.high_pass_hz === hz
                         ? 'border-violet-500/50 bg-violet-500/10 dark:bg-violet-500/15 text-violet-600 dark:text-violet-400'
                         : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
                {hz === null ? t("common.off") : `${hz} Hz`}
              </button>
            {/each}
          </div>
        </div>

        <!-- Low-pass -->
        <div class="flex flex-col gap-2 px-4 py-3.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.lowPassCutoff")}</span>
          <div class="flex items-center gap-1.5 flex-wrap">
            {#each ([null, 30, 50, 100] as const) as hz}
              <button onclick={() => setLowPass(hz)}
                class="rounded-lg border px-3 py-1.5 text-[0.68rem] font-semibold
                       transition-all cursor-pointer select-none
                       {filter.low_pass_hz === hz
                         ? 'border-violet-500/50 bg-violet-500/10 dark:bg-violet-500/15 text-violet-600 dark:text-violet-400'
                         : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
                {hz === null ? t("common.off") : `${hz} Hz`}
              </button>
            {/each}
          </div>
        </div>

        <!-- Pipeline summary -->
        <div class="flex items-center gap-2 flex-wrap px-4 py-3 bg-slate-50 dark:bg-[#111118]">
          <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground shrink-0">
            {t("settings.pipeline")}
          </span>
          {#if filter.high_pass_hz !== null}
            <Badge variant="outline"
              class="text-[0.56rem] py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
              HP {filter.high_pass_hz} Hz
            </Badge>
          {/if}
          {#if filter.low_pass_hz !== null}
            <Badge variant="outline"
              class="text-[0.56rem] py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
              LP {filter.low_pass_hz} Hz
            </Badge>
          {/if}
          {#if filter.notch !== null}
            <Badge variant="outline"
              class="text-[0.56rem] py-0 px-1.5 bg-primary/10 text-primary border-primary/20">
              Notch {filter.notch === "Hz60" ? "60+120 Hz" : "50+100 Hz"}
            </Badge>
          {/if}
          {#if filter.high_pass_hz === null && filter.low_pass_hz === null && filter.notch === null}
            <Badge variant="outline"
              class="text-[0.56rem] py-0 px-1.5 bg-slate-500/10 text-slate-500 border-slate-500/20">
              {t("settings.passthrough")}
            </Badge>
          {/if}
          <span class="ml-auto text-[0.56rem] text-muted-foreground/60 shrink-0">{t("settings.gpuLatency")}</span>
        </div>

      </CardContent>
    </Card>
  </div>

  <!-- ── EEG Embedding ───────────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <div class="flex items-center gap-2 px-0.5">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("settings.eegEmbedding")}
      </span>
      {#if overlapSaving}
        <span class="text-[0.56rem] text-muted-foreground">saving…</span>
      {/if}
    </div>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

        <div class="flex flex-col gap-2 px-4 py-3.5">
          <div class="flex items-baseline justify-between">
            <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.epochOverlap")}</span>
            <span class="text-[0.68rem] text-muted-foreground">
              {t("settings.everyNSecs", { n: (EMBEDDING_EPOCH_SECS - overlapSecs).toFixed(2).replace(/\.?0+$/, "") })}
            </span>
          </div>
          <p class="text-[0.68rem] text-muted-foreground leading-relaxed -mt-0.5">
            {t("settings.overlapDescription")}
          </p>
          <div class="flex items-center gap-1.5 flex-wrap">
            {#each OVERLAP_PRESETS as [label, val]}
              <button
                onclick={() => setOverlap(val)}
                class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                       transition-all cursor-pointer select-none
                       {overlapSecs === val
                         ? 'border-primary/50 bg-primary/10 text-primary'
                         : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
                {label}
              </button>
            {/each}
          </div>
        </div>

        <div class="flex items-center gap-2 flex-wrap px-4 py-3 bg-slate-50 dark:bg-[#111118]">
          <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground shrink-0">
            Pipeline
          </span>
          <Badge variant="outline"
            class="text-[0.56rem] py-0 px-1.5 bg-primary/10 text-primary border-primary/20">
            {EMBEDDING_EPOCH_SECS} s window
          </Badge>
          <Badge variant="outline"
            class="text-[0.56rem] py-0 px-1.5 bg-primary/10 text-primary border-primary/20">
            {overlapSecs} s overlap
          </Badge>
          <Badge variant="outline"
            class="text-[0.56rem] py-0 px-1.5 bg-primary/10 text-primary border-primary/20">
            {Math.round(overlapSecs / EMBEDDING_EPOCH_SECS * 100)}% shared
          </Badge>
          <span class="ml-auto text-[0.56rem] text-muted-foreground/60 shrink-0">ZUNA · wgpu</span>
        </div>

      </CardContent>
    </Card>
  </div>

  <!-- ── GPU / Memory ───────────────────────────────────────────────────────── -->
  {#if gpuStats}
    {@const fmtBytes = (b: number | null) => {
      if (b === null || b <= 0) return null;
      const gb = b / (1024 ** 3);
      return gb >= 1 ? `${gb.toFixed(1)} GB` : `${(b / (1024 ** 2)).toFixed(0)} MB`;
    }}
    {@const usedBytes  = (gpuStats.totalMemoryBytes !== null && gpuStats.freeMemoryBytes !== null)
      ? gpuStats.totalMemoryBytes - gpuStats.freeMemoryBytes : null}
    {@const usedPct    = (usedBytes !== null && gpuStats.totalMemoryBytes)
      ? Math.round(usedBytes / gpuStats.totalMemoryBytes * 100) : null}
    {@const memLabel   = gpuStats.isUnifiedMemory ? "Unified Memory (RAM)" : "VRAM"}

    <div class="flex flex-col gap-2">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
        GPU · {memLabel}
      </span>

      <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
        <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

          {#if gpuStats.totalMemoryBytes}
            <div class="flex flex-col gap-2 px-4 py-3.5">
              <div class="flex items-baseline justify-between">
                <span class="text-[0.72rem] font-semibold text-foreground">{memLabel}</span>
                {#if fmtBytes(gpuStats.totalMemoryBytes)}
                  <span class="text-[0.68rem] text-muted-foreground tabular-nums">
                    {fmtBytes(gpuStats.totalMemoryBytes)}
                    {#if gpuStats.isUnifiedMemory}<span class="text-[0.56rem] ml-0.5 text-muted-foreground/60">total</span>{/if}
                  </span>
                {/if}
              </div>

              {#if usedPct !== null && gpuStats.freeMemoryBytes !== null}
                <div class="h-2 w-full rounded-full bg-muted dark:bg-white/[0.07] overflow-hidden">
                  <div
                    class="h-full rounded-full transition-all duration-500
                           {usedPct > 85 ? 'bg-red-500' : usedPct > 65 ? 'bg-amber-500' : 'bg-violet-500'}"
                    style="width: {usedPct}%">
                  </div>
                </div>
                <div class="flex items-center justify-between text-[0.6rem] text-muted-foreground tabular-nums">
                  <span>
                    {fmtBytes(usedBytes)} used
                    <span class="text-muted-foreground/50">·</span>
                    {fmtBytes(gpuStats.freeMemoryBytes)} free
                  </span>
                  <span class="{usedPct > 85 ? 'text-red-500' : usedPct > 65 ? 'text-amber-500' : ''}">
                    {usedPct}%
                  </span>
                </div>
              {:else if gpuStats.freeMemoryBytes}
                <p class="text-[0.64rem] text-muted-foreground">
                  {fmtBytes(gpuStats.freeMemoryBytes)} free
                </p>
              {/if}

              {#if gpuStats.isUnifiedMemory}
                <p class="text-[0.58rem] text-muted-foreground/60 leading-relaxed -mt-0.5">
                  Apple Silicon uses a single unified memory pool shared by CPU and GPU.
                  "Free" includes inactive pages that can be reclaimed immediately.
                </p>
              {/if}
            </div>
          {/if}

          {#if gpuStats.overall > 0 || gpuStats.render > 0 || gpuStats.tiler > 0}
            <div class="flex items-center gap-4 px-4 py-3 bg-slate-50 dark:bg-[#111118]">
              <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground shrink-0">
                GPU Usage
              </span>
              {#each ([
                ["Render",  gpuStats.render],
                ["Tiler",   gpuStats.tiler],
                ["Overall", gpuStats.overall],
              ] as [string, number][]).filter(([, v]) => v > 0) as [label, val]}
                <div class="flex items-center gap-1.5">
                  <div class="h-1.5 w-16 rounded-full bg-muted dark:bg-white/[0.07] overflow-hidden">
                    <div class="h-full rounded-full bg-violet-500/70 transition-all"
                         style="width:{Math.round(val * 100)}%"></div>
                  </div>
                  <span class="text-[0.58rem] text-muted-foreground tabular-nums">
                    {label} {Math.round(val * 100)}%
                  </span>
                </div>
              {/each}
            </div>
          {/if}

        </CardContent>
      </Card>
    </div>
  {/if}

</section>

<!-- ── Device row snippet ──────────────────────────────────────────────────── -->
{#snippet deviceRow(dev: DiscoveredDevice)}
  {@const imgSrc = deviceImage(dev.name, dev.hardware_version)}
  <div class="flex items-center gap-3 px-4 py-3
              transition-colors hover:bg-slate-50 dark:hover:bg-white/[0.02]
              {dev.is_preferred ? 'bg-blue-50 dark:bg-blue-950/20' : ''}
              {!dev.is_paired ? 'opacity-80' : ''}">

    <!-- Device photo -->
    {#if imgSrc}
      <div class="w-12 h-12 rounded-lg shrink-0 overflow-hidden
                {imgSrc.endsWith('.png') || imgSrc.endsWith('.svg') ? 'bg-white' : 'bg-muted/40 dark:bg-white/[0.04]'}
                {!dev.is_paired ? 'grayscale opacity-60' : ''}">
        <img src={imgSrc} alt={dev.name} class="w-full h-full object-cover" />
      </div>
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
