<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Devices tab — paired/discovered devices, OpenBCI config, device API, scanner backends. -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { onDestroy, onMount } from "svelte";
import { Badge } from "$lib/components/ui/badge";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import {
  forgetDevice,
  getCortexWsState,
  getDeviceApiConfig,
  getDeviceLog,
  getDeviceStatus,
  getDevices,
  getOpenbciConfig,
  getScannerConfig,
  getWsPort,
  listSerialPorts,
  pairDevice as pairDeviceCmd,
  setDeviceApiConfig,
  setOpenbciConfig,
  setPreferredDevice,
  setScannerConfig,
} from "$lib/daemon/client";
import { applyPreferred } from "$lib/devices-logic";
import { t } from "$lib/i18n/index.svelte";
import { getSupportedCompanies, loadSupportedCompanies, type SupportedCompanyId } from "$lib/supported-devices";
import { colorForRssi } from "$lib/theme";

// ── Types ──────────────────────────────────────────────────────────────────
interface DiscoveredDevice {
  id: string;
  name: string;
  last_seen: number;
  last_rssi: number;
  is_paired: boolean;
  is_preferred: boolean;
  hardware_version?: string | null;
  transport?: "ble" | "usb_serial" | "wifi" | "cortex";
}
interface ConnectedInfo {
  device_id: string | null;
  serial_number: string | null;
  mac_address: string | null;
}
interface PairedDeviceInfo {
  id: string;
  name: string;
  last_seen: number;
}
interface StatusPayload extends ConnectedInfo {
  paired_devices?: PairedDeviceInfo[];
}

// ── State ──────────────────────────────────────────────────────────────────
let devices = $state<DiscoveredDevice[]>([]);
let pairedFromStatus = $state<PairedDeviceInfo[]>([]);
let connected = $state<ConnectedInfo>({ device_id: null, serial_number: null, mac_address: null });
let now = $state(Math.floor(Date.now() / 1000));
let revealSN = $state(false);
let revealMAC = $state(false);

// ── OpenBCI config ──────────────────────────────────────────────────────────
type OpenBciBoard =
  | "ganglion"
  | "ganglion_wifi"
  | "cyton"
  | "cyton_wifi"
  | "cyton_daisy"
  | "cyton_daisy_wifi"
  | "galea";
interface OpenBciConfig {
  board: OpenBciBoard;
  scan_timeout_secs: number;
  serial_port: string;
  wifi_shield_ip: string;
  wifi_local_port: number;
  galea_ip: string;
  channel_labels: string[];
}
interface DeviceApiConfig {
  emotiv_client_id: string;
  emotiv_client_secret: string;
  idun_api_token: string;
  oura_access_token: string;
  neurosity_email: string;
  neurosity_password: string;
  neurosity_device_id: string;
  brainmaster_model: string;
}
const OPENBCI_DEFAULT: OpenBciConfig = {
  board: "ganglion",
  scan_timeout_secs: 10,
  serial_port: "",
  wifi_shield_ip: "",
  wifi_local_port: 3000,
  galea_ip: "",
  channel_labels: [],
};
let openbci = $state<OpenBciConfig>({ ...OPENBCI_DEFAULT });
let openbciSaved = $state(false);
let openbciChanged = $state(false);
let openbciConnecting = $state(false);
let openbciError = $state("");
let openbciExpanded = $state(false);
let deviceApi = $state<DeviceApiConfig>({
  emotiv_client_id: "",
  emotiv_client_secret: "",
  idun_api_token: "",
  oura_access_token: "",
  neurosity_email: "",
  neurosity_password: "",
  neurosity_device_id: "",
  brainmaster_model: "atlantis4",
});
let emotivApiChanged = $state(false);
let emotivApiSaved = $state(false);
let emotivApiError = $state("");
let idunApiChanged = $state(false);
let idunApiSaved = $state(false);
let idunApiError = $state("");
let emotivSecretVisible = $state(false);
let idunTokenVisible = $state(false);
let emotivApiExpanded = $state(false);
let idunApiExpanded = $state(false);
let ouraApiChanged = $state(false);
let ouraApiSaved = $state(false);
let ouraApiError = $state("");
let ouraTokenVisible = $state(false);
let ouraApiExpanded = $state(false);
let ouraSyncing = $state(false);
let ouraSynced = $state(false);
let ouraSyncError = $state("");
let neurosityApiChanged = $state(false);
let neurosityApiSaved = $state(false);
let neurosityApiError = $state("");
let neurosityPasswordVisible = $state(false);
let neurosityApiExpanded = $state(false);
let brainmasterApiChanged = $state(false);
let brainmasterApiSaved = $state(false);
let brainmasterApiError = $state("");
let brainmasterApiExpanded = $state(false);
let supportedCompanies = $state(getSupportedCompanies());
let supportedCompanyExpanded = $state<SupportedCompanyId | null>(null);
let supportedDevicesSearchQuery = $state("");
let serialPorts = $state<string[]>([]);
let portsLoading = $state(false);

// ── Scanner config ──────────────────────────────────────────────────────────
interface ScannerConfig {
  ble: boolean;
  usb_serial: boolean;
  cortex: boolean;
}
let scannerConfig = $state<ScannerConfig>({ ble: true, usb_serial: true, cortex: true });
let scannerChanged = $state(false);
let scannerSaved = $state(false);

// ── Cortex WebSocket state ──────────────────────────────────────────────────
type CortexWsState = "disconnected" | "connecting" | "connected";
let cortexWsState = $state<CortexWsState>("disconnected");

// ── Device log ──────────────────────────────────────────────────────────────
interface DeviceLogEntry {
  ts: number;
  tag: string;
  msg: string;
}
let deviceLog = $state<DeviceLogEntry[]>([]);
let deviceLogExpanded = $state(false);
let deviceLogInterval: ReturnType<typeof setInterval> | null = null;
let devicePollInterval: ReturnType<typeof setInterval> | null = null;

// Fuzzy search: case-insensitive substring + character subsequence matching
function fuzzyMatch(haystack: string, needle: string): boolean {
  if (!needle) return true;
  const h = haystack.toLowerCase();
  const n = needle.toLowerCase();
  if (h.includes(n)) return true;
  let hIdx = 0;
  for (let i = 0; i < n.length; i++) {
    hIdx = h.indexOf(n[i], hIdx);
    if (hIdx === -1) return false;
    hIdx++;
  }
  return true;
}

const filteredCompanies = $derived(
  (() => {
    if (!supportedDevicesSearchQuery) return supportedCompanies;
    return supportedCompanies
      .map((company) => ({
        ...company,
        devices: company.devices.filter((device) => {
          const companyName = t(company.name_key);
          const deviceName = t(device.name_key);
          return (
            fuzzyMatch(companyName, supportedDevicesSearchQuery) || fuzzyMatch(deviceName, supportedDevicesSearchQuery)
          );
        }),
      }))
      .filter((company) => company.devices.length > 0);
  })(),
);

async function saveScannerConfig() {
  await setScannerConfig(scannerConfig);
  scannerChanged = false;
  scannerSaved = true;
  setTimeout(() => {
    scannerSaved = false;
  }, 2000);
}

async function refreshDeviceLog() {
  try {
    deviceLog = await getDeviceLog();
  } catch {
    /* noop */
  }
}

async function loadSerialPorts() {
  portsLoading = true;
  try {
    serialPorts = await listSerialPorts();
  } catch {
    serialPorts = [];
  }
  portsLoading = false;
}

async function saveOpenbci() {
  await setOpenbciConfig(openbci);
  openbciChanged = false;
  openbciSaved = true;
  setTimeout(() => {
    openbciSaved = false;
  }, 2000);
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
    await setDeviceApiConfig(deviceApi);
    emotivApiChanged = false;
    emotivApiSaved = true;
    setTimeout(() => {
      emotivApiSaved = false;
    }, 2000);
  } catch (e: unknown) {
    emotivApiError = e instanceof Error ? e.message : String(e);
  }
}

async function saveIdunApi() {
  idunApiError = "";
  try {
    await setDeviceApiConfig(deviceApi);
    idunApiChanged = false;
    idunApiSaved = true;
    setTimeout(() => {
      idunApiSaved = false;
    }, 2000);
  } catch (e: unknown) {
    idunApiError = e instanceof Error ? e.message : String(e);
  }
}

async function saveOuraApi() {
  ouraApiError = "";
  try {
    await setDeviceApiConfig(deviceApi);
    ouraApiChanged = false;
    ouraApiSaved = true;
    setTimeout(() => {
      ouraApiSaved = false;
    }, 2000);
  } catch (e: unknown) {
    ouraApiError = e instanceof Error ? e.message : String(e);
  }
}

async function saveNeurosityApi() {
  neurosityApiError = "";
  try {
    await setDeviceApiConfig(deviceApi);
    neurosityApiChanged = false;
    neurosityApiSaved = true;
    setTimeout(() => {
      neurosityApiSaved = false;
    }, 2000);
  } catch (e: unknown) {
    neurosityApiError = e instanceof Error ? e.message : String(e);
  }
}

async function saveBrainmasterApi() {
  brainmasterApiError = "";
  try {
    await setDeviceApiConfig(deviceApi);
    brainmasterApiChanged = false;
    brainmasterApiSaved = true;
    setTimeout(() => {
      brainmasterApiSaved = false;
    }, 2000);
  } catch (e: unknown) {
    brainmasterApiError = e instanceof Error ? e.message : String(e);
  }
}

async function ouraSync() {
  ouraSyncError = "";
  ouraSyncing = true;
  try {
    const now = new Date();
    const end = now.toISOString().split("T")[0];
    const start = new Date(now.getTime() - 30 * 86400 * 1000).toISOString().split("T")[0];
    const port = await getWsPort();
    const resp = await fetch(`http://127.0.0.1:${port}/`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ command: "oura_sync", start_date: start, end_date: end }),
      signal: AbortSignal.timeout(300000), // 5 min — Oura API can be slow for large ranges
    });
    if (!resp.ok) {
      ouraSyncError = `Server returned ${resp.status} ${resp.statusText}`;
      return;
    }
    const r = await resp.json().catch(() => null);
    if (!r) {
      ouraSyncError = "Invalid response from server";
      return;
    }
    if (r.ok) {
      ouraSynced = true;
      setTimeout(() => {
        ouraSynced = false;
      }, 3000);
    } else {
      ouraSyncError = r.error ?? "Sync failed";
    }
  } catch (e: unknown) {
    ouraSyncError = e instanceof Error ? e.message : String(e);
  } finally {
    ouraSyncing = false;
  }
}

const isBle = $derived(openbci.board === "ganglion");
const isSerial = $derived(openbci.board === "cyton" || openbci.board === "cyton_daisy");
const isWifi = $derived(["ganglion_wifi", "cyton_wifi", "cyton_daisy_wifi"].includes(openbci.board));
const isGalea = $derived(openbci.board === "galea");

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
  openbci.board === "cyton_daisy" || openbci.board === "cyton_daisy_wifi"
    ? 16
    : openbci.board === "cyton" || openbci.board === "cyton_wifi"
      ? 8
      : openbci.board === "galea"
        ? 24
        : 4,
);

// ── Channel label presets ─────────────────────────────────────────────────
type PresetMap = Record<string, { label: string; names: string[] }>;

const PRESETS_4CH: PresetMap = {
  default: { label: "OpenBCI defaults (Ch1-4)", names: ["Ch1", "Ch2", "Ch3", "Ch4"] },
  frontal: { label: "Frontal (Fp1, Fp2, F7, F8)", names: ["Fp1", "Fp2", "F7", "F8"] },
  motor: { label: "Motor (C3, Cz, C4, Fz)", names: ["C3", "Cz", "C4", "Fz"] },
  occipital: { label: "Occipital (O1, Oz, O2, Pz)", names: ["O1", "Oz", "O2", "Pz"] },
};
const PRESETS_8CH: PresetMap = {
  default: { label: "OpenBCI defaults (Fp1-O2)", names: ["Fp1", "Fp2", "C3", "C4", "P3", "P4", "O1", "O2"] },
  frontal: { label: "Frontal (8ch)", names: ["Fp1", "Fp2", "F3", "F4", "F7", "F8", "Fz", "AFz"] },
  motor: { label: "Motor strip (FC5-FC6 montage)", names: ["FC5", "FC3", "FC1", "FC2", "FC4", "FC6", "C3", "C4"] },
  temporal: { label: "Temporal (T7/T8 montage)", names: ["F7", "T7", "P7", "O1", "F8", "T8", "P8", "O2"] },
};
const PRESETS_16CH: PresetMap = {
  default: {
    label: "Full 10-20 (16ch)",
    names: ["Fp1", "Fp2", "F3", "F4", "C3", "C4", "P3", "P4", "O1", "O2", "F7", "F8", "T7", "T8", "Fz", "Pz"],
  },
  frontal: {
    label: "Bilateral frontal (16ch)",
    names: ["Fp1", "Fp2", "AF3", "AF4", "F3", "F4", "F7", "F8", "FC1", "FC2", "FC5", "FC6", "Fz", "AFz", "FT7", "FT8"],
  },
  motor: {
    label: "Full motor (16ch)",
    names: ["FC5", "FC3", "FC1", "FC2", "FC4", "FC6", "C5", "C3", "C1", "C2", "C4", "C6", "CP5", "CP3", "CP4", "CP6"],
  },
};
const PRESETS_24CH: PresetMap = {
  default: {
    label: "Galea defaults (EMG 0-7, EEG 8-17, AUX 18-21)",
    names: [
      "EMG1",
      "EMG2",
      "EMG3",
      "EMG4",
      "EMG5",
      "EMG6",
      "EMG7",
      "EMG8",
      "Fp1",
      "Fp2",
      "F3",
      "F4",
      "C3",
      "C4",
      "P3",
      "P4",
      "O1",
      "O2",
      "AUX1",
      "AUX2",
      "AUX3",
      "AUX4",
      "Rsv1",
      "Rsv2",
    ],
  },
  eeg_only: {
    label: "EEG channels only (label all as 10-20)",
    names: [
      "F7",
      "F3",
      "Fz",
      "F4",
      "F8",
      "C3",
      "Cz",
      "C4",
      "T7",
      "T8",
      "P7",
      "P3",
      "Pz",
      "P4",
      "P8",
      "O1",
      "Oz",
      "O2",
      "TP9",
      "TP10",
      "FT9",
      "FT10",
      "PO9",
      "PO10",
    ],
  },
};

const LABEL_PRESETS: Record<OpenBciBoard, PresetMap> = {
  ganglion: PRESETS_4CH,
  ganglion_wifi: PRESETS_4CH,
  cyton: PRESETS_8CH,
  cyton_wifi: PRESETS_8CH,
  cyton_daisy: PRESETS_16CH,
  cyton_daisy_wifi: PRESETS_16CH,
  galea: PRESETS_24CH,
};

const defaultChannelNames = $derived(Object.values(LABEL_PRESETS[openbci.board])[0]?.names ?? []);

const activePreset = $derived(
  (() => {
    const presets = LABEL_PRESETS[openbci.board];
    for (const [id, p] of Object.entries(presets)) {
      const matches =
        p.names.length === channelCount && p.names.every((n, i) => (openbci.channel_labels[i] ?? "") === n);
      if (matches) return id;
    }
    const allBlank = openbci.channel_labels.slice(0, channelCount).every((l) => !l);
    return allBlank ? "default" : "__custom__";
  })(),
);

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
const fmtRssi = (r: number) => (r === 0 ? "—" : `${r} dBm`);

function redact(v: string) {
  const parts = v.split("-");
  return [...parts.slice(0, -1).map((p) => "*".repeat(p.length)), parts.at(-1)].join("-");
}

function fmtLastSeen(ts: number) {
  if (ts === 0) return "never";
  const d = now - ts;
  if (d < 5) return "just now";
  if (d < 60) return `${d}s ago`;
  if (d < 3600) return `${Math.floor(d / 60)}m ago`;
  return `${Math.floor(d / 3600)}h ago`;
}

// ── Virtual device detection ──────────────────────────────────────────────────
function isVirtualDevice(dev: { id: string; name: string }): boolean {
  const n = dev.name.toLowerCase();
  const id = dev.id.toLowerCase();
  return n.includes("virtual") || id.includes("virtual");
}

function sortDevicesRealFirst<T extends { id: string; name: string }>(devs: T[]): T[] {
  return [...devs].sort((a, b) => {
    const av = isVirtualDevice(a) ? 1 : 0;
    const bv = isVirtualDevice(b) ? 1 : 0;
    return av - bv;
  });
}

// ── Device images ──────────────────────────────────────────────────────────
function museImage(name: string, _hw?: string | null): string | null {
  const n = name.toLowerCase();
  const isAthena = n.includes("muses");
  if (isAthena) return "/devices/muse-s-athena.jpg";
  if (n.includes("muse-s") || n.includes("muse s")) return "/devices/muse-s-gen1.jpg";
  if (n.includes("muse-2") || n.includes("muse2") || n.includes("muse 2")) return "/devices/muse-gen2.jpg";
  if (n.includes("muse")) return "/devices/muse-gen1.jpg";
  if (n.includes("mw75") || n.includes("neurable")) return "/devices/muse-mw75.jpg";
  return null;
}

function deviceImage(name: string, hw?: string | null): string | null {
  const muse = museImage(name, hw);
  if (muse) return muse;

  const n = name.toLowerCase();
  if (n.includes("awear") || n.includes("luca")) {
    return "/devices/awear-eeg.png";
  }
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
  if (n.includes("oura")) {
    return "/devices/oura-ring.svg";
  }

  return null;
}

const OPENBCI_IMAGES: Record<string, string> = {
  ganglion: "/devices/openbci-ganglion.jpg",
  ganglion_wifi: "/devices/openbci-ganglion-wifi.jpg",
  cyton: "/devices/openbci-cyton.png",
  cyton_wifi: "/devices/openbci-cyton-wifi.jpg",
  cyton_daisy: "/devices/openbci-cyton-daisy.jpg",
  cyton_daisy_wifi: "/devices/openbci-cyton-daisy-wifi.jpg",
  galea: "/devices/openbci-galea.jpg",
};

function inferTransport(id: string): DiscoveredDevice["transport"] {
  if (id.startsWith("usb:") || id === "neurosky") return "usb_serial";
  if (id.startsWith("wifi:") || id.startsWith("brainvision:")) return "wifi";
  if (id.startsWith("cortex:")) return "cortex";
  return "ble";
}

function mergePairedIntoDevices(base: DiscoveredDevice[], paired: PairedDeviceInfo[]): DiscoveredDevice[] {
  const out = [...base];
  const byId = new Set(out.map((d) => d.id));
  for (const p of paired) {
    if (!byId.has(p.id)) {
      out.push({
        id: p.id,
        name: p.name,
        last_seen: p.last_seen ?? 0,
        last_rssi: 0,
        is_paired: true,
        is_preferred: false,
        transport: inferTransport(p.id),
      });
      byId.add(p.id);
    }
  }
  return out;
}

function isManualHint(d: DiscoveredDevice): boolean {
  return d.id === "neurosky" || d.id === "brainvision:127.0.0.1:51244";
}

// ── Device lists ─────────────────────────────────────────────────────────────
const allDevices = $derived(mergePairedIntoDevices(devices, pairedFromStatus));
// Paired: real hardware always first, virtual devices at the bottom.
const pairedDevices = $derived(sortDevicesRealFirst(allDevices.filter((d) => d.is_paired)));
// Discovered: split so the template renders real devices above the virtual subsection.
const discoveredReal = $derived(allDevices.filter((d) => !d.is_paired && !isVirtualDevice(d) && !isManualHint(d)));
const discoveredVirtual = $derived(allDevices.filter((d) => !d.is_paired && isVirtualDevice(d)));
const manualHintDevices = $derived(allDevices.filter((d) => !d.is_paired && isManualHint(d)));
const discoveredDevices = $derived([...discoveredReal, ...discoveredVirtual, ...manualHintDevices]);
// "New device" banner only fires for real hardware — not virtual sources.
const newUnpairedDevices = $derived(
  allDevices.filter((d) => !d.is_paired && d.last_rssi !== 0 && !isVirtualDevice(d) && !isManualHint(d)),
);
const hasNewUnpaired = $derived(newUnpairedDevices.length > 0);

function expandSupportedCompany(id: SupportedCompanyId) {
  supportedCompanyExpanded = supportedCompanyExpanded === id ? null : id;
  if (id === "openbci") openbciExpanded = true;
  if (id === "emotiv") emotivApiExpanded = true;
  if (id === "idun") idunApiExpanded = true;
  if (id === "oura") ouraApiExpanded = true;
}

// ── Device actions ─────────────────────────────────────────────────────────
async function setPreferred(id: string) {
  const cur = devices.find((d) => d.id === id);
  const targetId = cur?.is_preferred ? "" : id;
  // Optimistically update UI immediately.
  devices = applyPreferred(devices, targetId);
  try {
    devices = await setPreferredDevice<DiscoveredDevice>(targetId);
  } catch {
    // Daemon HTTP may fail (e.g. auth token mismatch after restart).
    // Fall back to Tauri invoke which has its own daemon call path
    // and applies the change locally even if the daemon is unreachable.
    try {
      devices = await invoke<DiscoveredDevice[]>("set_preferred_device", { id: targetId });
    } catch {
      // Both paths failed — optimistic update still visible.
    }
  }
}
async function forget(id: string) {
  await forgetDevice(id);
  devices = devices.map((d) => (d.id === id ? { ...d, is_paired: false } : d));
  pairedFromStatus = pairedFromStatus.filter((p) => p.id !== id);
}
async function pairDevice(id: string) {
  devices = await pairDeviceCmd<DiscoveredDevice>(id);
  try {
    const status = await getDeviceStatus<StatusPayload>();
    pairedFromStatus = status.paired_devices ?? pairedFromStatus;
  } catch {
    /* noop */
  }
}

// ── Lifecycle ──────────────────────────────────────────────────────────────
let unlisteners: UnlistenFn[] = [];
let nowTimer: ReturnType<typeof setInterval>;

onMount(async () => {
  supportedCompanies = await loadSupportedCompanies();
  devices = await getDevices();
  const status = await getDeviceStatus<StatusPayload>();
  pairedFromStatus = status.paired_devices ?? [];
  openbci = await getOpenbciConfig();
  deviceApi = await getDeviceApiConfig();
  scannerConfig = await getScannerConfig();
  cortexWsState = await getCortexWsState<CortexWsState>();
  await loadSerialPorts();
  await refreshDeviceLog();
  deviceLogInterval = setInterval(refreshDeviceLog, 3000);
  devicePollInterval = setInterval(async () => {
    try {
      devices = await getDevices();
    } catch (_) {
      /* daemon may be unreachable */
    }
  }, 3000);

  nowTimer = setInterval(() => (now = Math.floor(Date.now() / 1000)), 1000);

  unlisteners.push(
    await listen<DiscoveredDevice[]>("devices-updated", (ev) => {
      devices = ev.payload;
    }),
    await listen<StatusPayload>("status", (ev) => {
      connected = {
        device_id: ev.payload.device_id ?? null,
        serial_number: ev.payload.serial_number ?? null,
        mac_address: ev.payload.mac_address ?? null,
      };
      pairedFromStatus = ev.payload.paired_devices ?? pairedFromStatus;
    }),
    await listen<CortexWsState>("cortex-ws-state", (ev) => {
      cortexWsState = ev.payload;
    }),
  );
});
onDestroy(() => {
  // biome-ignore lint/suspicious/useIterableCallbackReturn: unlisten fns return void-Promise, not a value
  unlisteners.forEach((u) => u());
  clearInterval(nowTimer);
  if (deviceLogInterval) clearInterval(deviceLogInterval);
  if (devicePollInterval) clearInterval(devicePollInterval);
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
          {#if i > 0 && !isVirtualDevice(pairedDevices[i - 1]) && isVirtualDevice(dev)}
            <!-- Divider between real hardware and virtual devices -->
            <div class="flex items-center gap-2 px-4 py-1.5 bg-muted/30 dark:bg-white/[0.02]
                        border-y border-border dark:border-white/[0.05]">
              <span class="text-[0.46rem] font-bold tracking-widest uppercase
                           text-muted-foreground/50">🔬 {t("devices.virtualDevices")}</span>
            </div>
          {:else if i > 0}
            <Separator class="bg-border dark:bg-white/[0.04]" />
          {/if}
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
        <!-- Real hardware -->
        {#each discoveredReal as dev, i (dev.id)}
          {#if i > 0}<Separator class="bg-border dark:bg-white/[0.04]" />{/if}
          {@render deviceRow(dev)}
        {/each}

        <!-- Virtual subsection -->
        {#if discoveredVirtual.length > 0}
          {#if discoveredReal.length > 0}
            <Separator class="bg-border dark:bg-white/[0.04]" />
          {/if}
          <!-- Subsection header -->
          <div class="flex items-center gap-2 px-4 py-2 bg-muted/30 dark:bg-white/[0.02]
                      {discoveredReal.length > 0 ? 'border-t border-border dark:border-white/[0.05]' : ''}">
            <span class="text-[0.46rem] font-bold tracking-widest uppercase text-muted-foreground/50">
              🔬 {t("devices.virtualDevices")}
            </span>
            <span class="text-[0.46rem] text-muted-foreground/35 leading-relaxed">— {t("devices.virtualDevicesHint")}</span>
          </div>
          {#each discoveredVirtual as dev, i (dev.id)}
            {#if i > 0}<Separator class="bg-border dark:bg-white/[0.04]" />{/if}
            {@render deviceRow(dev)}
          {/each}
        {/if}

        <!-- Manual connection hints subsection -->
        {#if manualHintDevices.length > 0}
          {#if discoveredReal.length > 0 || discoveredVirtual.length > 0}
            <Separator class="bg-border dark:bg-white/[0.04]" />
          {/if}
          <div class="flex items-center gap-2 px-4 py-2 bg-muted/30 dark:bg-white/[0.02]
                      {(discoveredReal.length > 0 || discoveredVirtual.length > 0)
                        ? 'border-t border-border dark:border-white/[0.05]'
                        : ''}">
            <span class="text-[0.46rem] font-bold tracking-widest uppercase text-muted-foreground/50">
              🧭 {t("devices.manualHints")}
            </span>
            <span class="text-[0.46rem] text-muted-foreground/35 leading-relaxed">— {t("devices.manualHintsHint")}</span>
          </div>
          {#each manualHintDevices as dev, i (dev.id)}
            {#if i > 0}<Separator class="bg-border dark:bg-white/[0.04]" />{/if}
            {@render deviceRow(dev)}
          {/each}
        {/if}
      {/if}
    </Card>
  </div>

  <!-- ── Supported Devices ─────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("settings.supportedDevices.title")}
    </span>

    <div class="flex flex-col gap-2">
      <input
        type="text"
        bind:value={supportedDevicesSearchQuery}
        aria-label={t("settings.supportedDevices.search")}
        placeholder={t("settings.supportedDevices.search")}
        class="text-[0.73rem] px-2.5 py-1.5 rounded-md border border-border bg-background text-foreground placeholder-muted-foreground focus:outline-none focus:ring-1 focus:ring-muted-foreground/50" />
    </div>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">
        {#each filteredCompanies as company (company.id)}
          <div class="flex flex-col">
            <button
              onclick={() => expandSupportedCompany(company.id)}
              class="flex items-center gap-3 w-full px-3.5 py-2.5 hover:bg-muted/30 transition-colors"
              aria-expanded={supportedCompanyExpanded === company.id}
            >
              <!-- Company logo -->
              <div class="w-8 h-8 rounded-md overflow-hidden shrink-0 bg-white flex items-center justify-center">
                <img src={company.logo} alt={t(company.name_key)} class="w-full h-full object-contain" />
              </div>
              <div class="flex flex-col items-start gap-0 flex-1 min-w-0">
                <span class="text-[0.7rem] font-semibold text-foreground">{t(company.name_key)}</span>
                <span class="text-[0.56rem] text-muted-foreground/60">
                  {company.devices.length} {company.devices.length === 1 ? t("devices.deviceSingular") : t("devices.devicePlural")}
                </span>
              </div>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                   class="w-3 h-3 text-muted-foreground/50 transition-transform duration-200
                          {supportedCompanyExpanded === company.id ? 'rotate-180' : ''}">
                <path d="M6 9l6 6 6-6"/>
              </svg>
            </button>

            {#if supportedCompanyExpanded === company.id}
              <div class="px-3.5 pb-3 flex flex-col gap-2">
                <div class="grid grid-cols-3 sm:grid-cols-4 lg:grid-cols-5 gap-1.5">
                  {#each company.devices as item (item.name_key)}
                    <div
                      class="flex flex-col items-stretch gap-0.5 rounded-md border border-border/50
                             dark:border-white/[0.05] bg-background/60 px-1.5 py-1.5"
                    >
                      <div class="w-full h-10 rounded overflow-hidden">
                        <img src={item.image} alt={t(item.name_key)} class="w-full h-full object-cover" />
                      </div>
                      <span class="text-[0.52rem] text-center leading-tight text-foreground/75 truncate">{t(item.name_key)}</span>
                      {#if item.ios_only}
                        <span class="text-[0.42rem] text-center leading-tight text-blue-500 dark:text-blue-400 font-semibold">📱 {t("devices.iosOnly")}</span>
                      {/if}
                    </div>
                  {/each}
                </div>

                <div class="rounded-md border border-border/50 dark:border-white/[0.05] bg-muted/30 px-2.5 py-2">
                  <p class="text-[0.6rem] font-medium text-foreground/80 mb-0.5">{t("settings.supportedDevices.howToConnect")}</p>
                  <div class="flex flex-col gap-0.5">
                    {#each company.instruction_keys as lineKey (lineKey)}
                      <p class="text-[0.56rem] text-muted-foreground leading-relaxed">• {t(lineKey)}</p>
                    {/each}
                  </div>
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </CardContent>
    </Card>
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

        <!-- ── OpenBCI sub-section ──────────────────────────────────────── -->
        <button
          onclick={() => openbciExpanded = !openbciExpanded}
          class="flex items-center justify-between w-full px-0.5 group"
          aria-expanded={openbciExpanded}
        >
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.deviceApi.openbciTitle")}</span>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
               stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
               class="w-3 h-3 text-muted-foreground/50 transition-transform duration-200
                      {openbciExpanded ? 'rotate-180' : ''}">
            <path d="M6 9l6 6 6-6"/>
          </svg>
        </button>

        {#if openbciExpanded}
          <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
            {t("settings.deviceApi.openbciDesc")}
          </p>

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
                  <select bind:value={openbci.serial_port} aria-label={t("settings.openbciSerialPort")} onchange={() => { openbciChanged = true; }}
                    class="flex-1 min-w-0 text-[0.73rem] px-2 py-1 rounded-md border border-border bg-background text-foreground">
                    <option value="">{t("settings.openbciSerialPortPlaceholder")}</option>
                    {#each serialPorts as p}<option value={p}>{p}</option>{/each}
                  </select>
                {:else}
                  <input type="text" bind:value={openbci.serial_port} aria-label={t("settings.openbciSerialPort")} oninput={() => { openbciChanged = true; }}
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
              <input type="text" bind:value={openbci.wifi_shield_ip} aria-label={t("settings.openbciWifiShieldIp")} oninput={() => { openbciChanged = true; }}
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
              <input type="text" bind:value={openbci.galea_ip} aria-label={t("settings.openbciGaleaIp")} oninput={() => { openbciChanged = true; }}
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
                aria-label="Channel preset"
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
                    aria-label={`Channel ${i + 1} label`}
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

          <!-- Save + Connect -->
          <div class="flex items-center justify-end gap-2">
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
            <p class="text-[0.62rem] text-destructive">{openbciError}</p>
          {/if}
        {/if}

        <Separator class="bg-border dark:bg-white/[0.04]" />

        <!-- ── Emotiv sub-section ───────────────────────────────────────── -->
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

        <Separator class="bg-border dark:bg-white/[0.04]" />

        <button
          onclick={() => neurosityApiExpanded = !neurosityApiExpanded}
          class="flex items-center justify-between w-full px-0.5 group"
          aria-expanded={neurosityApiExpanded}
        >
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.deviceApi.neurosityTitle")}</span>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
               stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
               class="w-3 h-3 text-muted-foreground/50 transition-transform duration-200
                      {neurosityApiExpanded ? 'rotate-180' : ''}">
            <path d="M6 9l6 6 6-6"/>
          </svg>
        </button>

        {#if neurosityApiExpanded}
          <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
            {t("settings.deviceApi.neurosityDesc")}
          </p>

          <div class="flex flex-col gap-1.5">
            <label for="neurosity-email" class="text-[0.68rem] font-medium text-foreground/80">{t("settings.deviceApi.neurosityEmail")}</label>
            <input
              id="neurosity-email"
              type="text"
              bind:value={deviceApi.neurosity_email}
              oninput={() => {
                neurosityApiChanged = true;
              }}
              placeholder="you@example.com"
              class="text-[0.73rem] px-2 py-1 rounded-md border border-border bg-background text-foreground" />
          </div>

          <div class="flex flex-col gap-1.5">
            <label for="neurosity-password" class="text-[0.68rem] font-medium text-foreground/80">{t("settings.deviceApi.neurosityPassword")}</label>
            <div class="flex items-center gap-2">
              <input
                id="neurosity-password"
                type={neurosityPasswordVisible ? "text" : "password"}
                bind:value={deviceApi.neurosity_password}
                oninput={() => {
                  neurosityApiChanged = true;
                }}
                placeholder="Neurosity account password"
                class="flex-1 min-w-0 text-[0.73rem] px-2 py-1 rounded-md border border-border bg-background text-foreground" />
              <Button
                size="sm"
                variant="outline"
                class="text-[0.64rem] h-7 px-2.5 shrink-0 border-border dark:border-white/10"
                onclick={() => (neurosityPasswordVisible = !neurosityPasswordVisible)}>
                {neurosityPasswordVisible ? t("settings.deviceApi.hide") : t("settings.deviceApi.show")}
              </Button>
            </div>
          </div>

          <div class="flex flex-col gap-1.5">
            <label for="neurosity-device-id" class="text-[0.68rem] font-medium text-foreground/80">{t("settings.deviceApi.neurosityDeviceId")}</label>
            <input
              id="neurosity-device-id"
              type="text"
              bind:value={deviceApi.neurosity_device_id}
              oninput={() => {
                neurosityApiChanged = true;
              }}
              placeholder="e.g. crown-xxxx"
              class="text-[0.73rem] px-2 py-1 rounded-md border border-border bg-background text-foreground" />
          </div>

          <div class="flex justify-end">
            <Button
              size="sm"
              variant={neurosityApiSaved ? "secondary" : "outline"}
              class="text-[0.66rem] h-7 px-3
                {neurosityApiSaved
                ? 'text-green-600 dark:text-green-400 border-green-500/30'
                : neurosityApiChanged
                  ? 'border-primary/50 text-primary'
                  : 'border-border dark:border-white/10 text-muted-foreground'}"
              onclick={saveNeurosityApi}
              disabled={!neurosityApiChanged && !neurosityApiSaved}>
              {neurosityApiSaved ? t("settings.deviceApi.saved") : t("settings.deviceApi.save")}
            </Button>
          </div>
          {#if neurosityApiError}
            <p class="text-[0.62rem] text-destructive">{neurosityApiError}</p>
          {/if}
        {/if}

        <Separator class="bg-border dark:bg-white/[0.04]" />

        <button
          onclick={() => brainmasterApiExpanded = !brainmasterApiExpanded}
          class="flex items-center justify-between w-full px-0.5 group"
          aria-expanded={brainmasterApiExpanded}
        >
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.deviceApi.brainmasterTitle")}</span>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
               stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
               class="w-3 h-3 text-muted-foreground/50 transition-transform duration-200
                      {brainmasterApiExpanded ? 'rotate-180' : ''}">
            <path d="M6 9l6 6 6-6"/>
          </svg>
        </button>

        {#if brainmasterApiExpanded}
          <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
            {t("settings.deviceApi.brainmasterDesc")}
          </p>

          <div class="flex flex-col gap-1.5">
            <label for="brainmaster-model" class="text-[0.68rem] font-medium text-foreground/80">{t("settings.deviceApi.brainmasterModel")}</label>
            <select
              id="brainmaster-model"
              bind:value={deviceApi.brainmaster_model}
              onchange={() => {
                brainmasterApiChanged = true;
              }}
              class="text-[0.73rem] px-2 py-1 rounded-md border border-border bg-background text-foreground">
              <option value="atlantis4">Atlantis 4</option>
              <option value="atlantis2">Atlantis 2</option>
              <option value="discovery">Discovery</option>
              <option value="freedom">Freedom</option>
            </select>
          </div>

          <div class="flex justify-end">
            <Button
              size="sm"
              variant={brainmasterApiSaved ? "secondary" : "outline"}
              class="text-[0.66rem] h-7 px-3
                {brainmasterApiSaved
                ? 'text-green-600 dark:text-green-400 border-green-500/30'
                : brainmasterApiChanged
                  ? 'border-primary/50 text-primary'
                  : 'border-border dark:border-white/10 text-muted-foreground'}"
              onclick={saveBrainmasterApi}
              disabled={!brainmasterApiChanged && !brainmasterApiSaved}>
              {brainmasterApiSaved ? t("settings.deviceApi.saved") : t("settings.deviceApi.save")}
            </Button>
          </div>
          {#if brainmasterApiError}
            <p class="text-[0.62rem] text-destructive">{brainmasterApiError}</p>
          {/if}
        {/if}

        <Separator class="bg-border dark:bg-white/[0.04]" />

        <button
          onclick={() => ouraApiExpanded = !ouraApiExpanded}
          class="flex items-center justify-between w-full px-0.5 group"
          aria-expanded={ouraApiExpanded}
        >
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.deviceApi.ouraTitle")}</span>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
               stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
               class="w-3 h-3 text-muted-foreground/50 transition-transform duration-200
                      {ouraApiExpanded ? 'rotate-180' : ''}">
            <path d="M6 9l6 6 6-6"/>
          </svg>
        </button>

        {#if ouraApiExpanded}
          <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
            {t("settings.deviceApi.ouraDesc")}
          </p>
          <a
            href="https://cloud.ouraring.com/personal-access-tokens"
            target="_blank"
            rel="noopener noreferrer"
            class="text-[0.62rem] text-primary hover:underline w-fit">
            {t("settings.deviceApi.ouraDashboard")}
          </a>

          <div class="flex flex-col gap-1.5">
            <label for="oura-access-token" class="text-[0.68rem] font-medium text-foreground/80">{t("settings.deviceApi.ouraAccessToken")}</label>
            <div class="flex items-center gap-2">
              <input
                id="oura-access-token"
                type={ouraTokenVisible ? "text" : "password"}
                bind:value={deviceApi.oura_access_token}
                oninput={() => { ouraApiChanged = true; }}
                placeholder="Oura Personal Access Token"
                class="flex-1 min-w-0 text-[0.73rem] px-2 py-1 rounded-md border border-border bg-background text-foreground" />
              <Button size="sm" variant="outline"
                class="text-[0.64rem] h-7 px-2.5 shrink-0 border-border dark:border-white/10"
                onclick={() => ouraTokenVisible = !ouraTokenVisible}>
                {ouraTokenVisible ? t("settings.deviceApi.hide") : t("settings.deviceApi.show")}
              </Button>
            </div>
          </div>

          <div class="flex items-center justify-between gap-2">
            <Button size="sm"
              variant={ouraSynced ? "secondary" : "outline"}
              class="text-[0.66rem] h-7 px-3
                {ouraSynced ? 'text-green-600 dark:text-green-400 border-green-500/30' :
                ouraSyncing ? 'border-yellow-500/30 text-yellow-600 dark:text-yellow-400' :
                'border-border dark:border-white/10 text-muted-foreground'}"
              onclick={ouraSync}
              disabled={ouraSyncing || !deviceApi.oura_access_token}>
              {#if ouraSyncing}
                {t("settings.deviceApi.ouraSyncing")}
              {:else if ouraSynced}
                {t("settings.deviceApi.ouraSynced")}
              {:else}
                {t("settings.deviceApi.ouraSyncBtn")}
              {/if}
            </Button>
            <Button size="sm"
              variant={ouraApiSaved ? "secondary" : "outline"}
              class="text-[0.66rem] h-7 px-3
                {ouraApiSaved ? 'text-green-600 dark:text-green-400 border-green-500/30' :
                ouraApiChanged ? 'border-primary/50 text-primary' :
                'border-border dark:border-white/10 text-muted-foreground'}"
              onclick={saveOuraApi}
              disabled={!ouraApiChanged && !ouraApiSaved}>
              {ouraApiSaved ? t("settings.deviceApi.saved") : t("settings.deviceApi.save")}
            </Button>
          </div>
          <p class="text-[0.58rem] text-muted-foreground/60 leading-relaxed">
            {t("settings.deviceApi.ouraSyncDesc")}
          </p>
          {#if ouraApiError}
            <p class="text-[0.62rem] text-destructive">{ouraApiError}</p>
          {/if}
          {#if ouraSyncError}
            <p class="text-[0.62rem] text-destructive">{ouraSyncError}</p>
          {/if}
        {/if}
      </CardContent>
    </Card>
  </div>

  <!-- ── Scanner Backends ──────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("settings.scanner.title")}
    </span>
    <p class="text-[0.62rem] text-muted-foreground/70 px-0.5 leading-relaxed">
      {t("settings.scanner.desc")}
    </p>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

        <!-- BLE -->
        <label class="flex items-center gap-3 px-4 py-3 cursor-pointer hover:bg-muted/30">
          <input type="checkbox" bind:checked={scannerConfig.ble}
            onchange={() => { scannerChanged = true; }}
            class="accent-violet-500 shrink-0" />
          <div class="flex flex-col gap-0.5 flex-1 min-w-0">
            <span class="text-[0.73rem] font-semibold text-foreground">{t("settings.scanner.ble")}</span>
            <span class="text-[0.6rem] text-muted-foreground">{t("settings.scanner.bleDesc")}</span>
          </div>
          <Badge variant="outline"
            class="text-[0.5rem] py-0 px-1.5 shrink-0
                   bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20">
            BLE
          </Badge>
        </label>

        <!-- USB Serial -->
        <label class="flex items-center gap-3 px-4 py-3 cursor-pointer hover:bg-muted/30">
          <input type="checkbox" bind:checked={scannerConfig.usb_serial}
            onchange={() => { scannerChanged = true; }}
            class="accent-violet-500 shrink-0" />
          <div class="flex flex-col gap-0.5 flex-1 min-w-0">
            <span class="text-[0.73rem] font-semibold text-foreground">{t("settings.scanner.usbSerial")}</span>
            <span class="text-[0.6rem] text-muted-foreground">{t("settings.scanner.usbSerialDesc")}</span>
          </div>
          <Badge variant="outline"
            class="text-[0.5rem] py-0 px-1.5 shrink-0
                   bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20">
            USB
          </Badge>
        </label>

        <!-- Cortex -->
        <label class="flex items-center gap-3 px-4 py-3 cursor-pointer hover:bg-muted/30">
          <input type="checkbox" bind:checked={scannerConfig.cortex}
            onchange={() => { scannerChanged = true; }}
            class="accent-violet-500 shrink-0" />
          <div class="flex flex-col gap-0.5 flex-1 min-w-0">
            <span class="text-[0.73rem] font-semibold text-foreground">{t("settings.scanner.cortex")}</span>
            <span class="text-[0.6rem] text-muted-foreground">{t("settings.scanner.cortexDesc")}</span>
          </div>
          {#if scannerConfig.cortex && deviceApi.emotiv_client_id && deviceApi.emotiv_client_secret}
            {#if cortexWsState === "connected"}
              <Badge variant="outline"
                class="text-[0.5rem] py-0 px-1.5 shrink-0 flex items-center gap-1
                       bg-green-500/10 text-green-600 dark:text-green-400 border-green-500/20">
                <span class="relative flex h-1.5 w-1.5">
                  <span class="absolute inline-flex h-full w-full rounded-full bg-green-500 opacity-75"></span>
                  <span class="relative inline-flex rounded-full h-1.5 w-1.5 bg-green-500"></span>
                </span>
                {t("settings.scanner.cortexConnected")}
              </Badge>
            {:else if cortexWsState === "connecting"}
              <Badge variant="outline"
                class="text-[0.5rem] py-0 px-1.5 shrink-0 flex items-center gap-1
                       bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20">
                <span class="relative flex h-1.5 w-1.5">
                  <span class="absolute inline-flex h-full w-full rounded-full bg-amber-500 opacity-75 animate-ping"></span>
                  <span class="relative inline-flex rounded-full h-1.5 w-1.5 bg-amber-500"></span>
                </span>
                {t("settings.scanner.cortexConnecting")}
              </Badge>
            {:else}
              <Badge variant="outline"
                class="text-[0.5rem] py-0 px-1.5 shrink-0 flex items-center gap-1
                       bg-red-500/10 text-red-600 dark:text-red-400 border-red-500/20">
                <span class="relative flex h-1.5 w-1.5">
                  <span class="relative inline-flex rounded-full h-1.5 w-1.5 bg-red-500"></span>
                </span>
                {t("settings.scanner.cortexDisconnected")}
              </Badge>
            {/if}
          {:else}
            <Badge variant="outline"
              class="text-[0.5rem] py-0 px-1.5 shrink-0
                     bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
              WS
            </Badge>
          {/if}
        </label>
      </CardContent>
    </Card>

    <div class="flex justify-end px-0.5">
      <Button size="sm"
        variant={scannerSaved ? "secondary" : "outline"}
        class="text-[0.66rem] h-7 px-3
               {scannerSaved ? 'text-green-600 dark:text-green-400 border-green-500/30' :
                scannerChanged ? 'border-primary/50 text-primary' :
                'border-border dark:border-white/10 text-muted-foreground'}"
        onclick={saveScannerConfig}
        disabled={!scannerChanged && !scannerSaved}>
        {scannerSaved ? t("settings.scanner.saved") : t("settings.scanner.save")}
      </Button>
    </div>
  </div>

  <!-- ── Device Log ──────────────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <button
      onclick={() => { deviceLogExpanded = !deviceLogExpanded; if (deviceLogExpanded) refreshDeviceLog(); }}
      class="flex items-center justify-between w-full px-0.5 group"
      aria-expanded={deviceLogExpanded}
    >
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("settings.deviceLog.title")}
      </span>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
           stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
           class="w-3 h-3 text-muted-foreground/50 transition-transform duration-200
                  {deviceLogExpanded ? 'rotate-180' : ''}">
        <path d="M6 9l6 6 6-6"/>
      </svg>
    </button>

    {#if deviceLogExpanded}
      <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
        <CardContent class="p-0">
          {#if deviceLog.length === 0}
            <p class="text-[0.64rem] text-muted-foreground/50 text-center py-6">
              {t("settings.deviceLog.empty")}
            </p>
          {:else}
            <div class="max-h-[260px] overflow-y-auto font-mono text-[0.58rem] leading-relaxed">
              {#each deviceLog.toReversed() as entry (entry.ts + entry.tag + entry.msg)}
                <div class="flex gap-2 px-3 py-1 border-b border-border/30 dark:border-white/[0.03]
                            hover:bg-muted/20 transition-colors">
                  <span class="text-muted-foreground/50 tabular-nums shrink-0 w-[52px]">
                    {new Date(entry.ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                  </span>
                  <span class="shrink-0 w-[52px] font-semibold
                    {entry.tag === 'ble'     ? 'text-blue-500' :
                     entry.tag === 'usb'     ? 'text-amber-500' :
                     entry.tag === 'cortex'  ? 'text-violet-500' :
                     entry.tag === 'session' ? 'text-green-500' :
                     'text-muted-foreground'}">
                    {entry.tag}
                  </span>
                  <span class="text-foreground/80 break-all min-w-0">{entry.msg}</span>
                </div>
              {/each}
            </div>
          {/if}
        </CardContent>
      </Card>
    {/if}
  </div>

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
        {#if isVirtualDevice(dev)}
          <Badge variant="outline"
            class="text-[0.46rem] tracking-wide uppercase py-0 px-1 shrink-0
                   bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 border-indigo-500/20">
            🔬 {t("devices.virtualBadge")}
          </Badge>
        {:else if dev.transport && dev.transport !== "ble"}
          <Badge variant="outline"
            class="text-[0.46rem] tracking-wide uppercase py-0 px-1 shrink-0
                   {dev.transport === 'usb_serial' ? 'bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20' :
                    dev.transport === 'cortex'     ? 'bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20' :
                    dev.transport === 'wifi'        ? 'bg-cyan-500/10 text-cyan-600 dark:text-cyan-400 border-cyan-500/20' :
                    'bg-slate-500/10 text-muted-foreground border-slate-500/20'}">
            {dev.transport === "usb_serial" ? "USB" :
             dev.transport === "cortex"     ? "Cortex" :
             dev.transport === "wifi"        ? "WiFi" : dev.transport}
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

      {#if dev.transport === "cortex"}
        <span class="text-[0.58rem] text-violet-600/80 dark:text-violet-400/70 leading-tight mt-0.5">
          {t("settings.emotivLauncherHint")}
        </span>
      {/if}

      {#if dev.id === connected.device_id && (connected.serial_number || connected.mac_address)}
        <div class="flex items-center gap-3 flex-wrap">
          {#if connected.serial_number}
            <button
              onclick={() => revealSN = !revealSN}
              title={revealSN ? t("common.clickToHide") : t("common.clickToReveal")}
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
              title={revealMAC ? t("common.clickToHide") : t("common.clickToReveal")}
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
