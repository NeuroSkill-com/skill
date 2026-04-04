// SPDX-License-Identifier: GPL-3.0-only

import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { daemonGet, daemonPost, getDaemonPort } from "./http";

export interface DiscoveredDevice {
  id: string;
  name: string;
  last_seen: number;
  last_rssi: number;
  is_paired: boolean;
  is_preferred: boolean;
  hardware_version?: string | null;
  transport?: "ble" | "usb_serial" | "wifi" | "cortex";
}

export interface DeviceLogEntry {
  ts: number;
  tag: string;
  msg: string;
}

export interface OpenBciConfig {
  board: "ganglion" | "ganglion_wifi" | "cyton" | "cyton_wifi" | "cyton_daisy" | "cyton_daisy_wifi" | "galea";
  scan_timeout_secs: number;
  serial_port: string;
  wifi_shield_ip: string;
  wifi_local_port: number;
  galea_ip: string;
  channel_labels: string[];
}

export interface DeviceApiConfig {
  emotiv_client_id: string;
  emotiv_client_secret: string;
  idun_api_token: string;
  oura_access_token: string;
}

export interface ScannerConfig {
  ble: boolean;
  usb_serial: boolean;
  cortex: boolean;
}

export function getDevices(): Promise<DiscoveredDevice[]> {
  return daemonGet<DiscoveredDevice[]>("/v1/devices");
}

export function getOpenbciConfig(): Promise<OpenBciConfig> {
  return daemonGet<OpenBciConfig>("/v1/settings/openbci-config");
}

export async function setOpenbciConfig(config: OpenBciConfig): Promise<void> {
  await daemonPost("/v1/settings/openbci-config", config);
}

export function getDeviceApiConfig(): Promise<DeviceApiConfig> {
  return daemonGet<DeviceApiConfig>("/v1/settings/device-api-config");
}

export async function setDeviceApiConfig(config: DeviceApiConfig): Promise<void> {
  await daemonPost("/v1/settings/device-api-config", config);
}

export function getScannerConfig(): Promise<ScannerConfig> {
  return daemonGet<ScannerConfig>("/v1/settings/scanner-config");
}

export async function setScannerConfig(config: ScannerConfig): Promise<void> {
  await daemonPost("/v1/settings/scanner-config", config);
}

export function getDeviceLog(): Promise<DeviceLogEntry[]> {
  return daemonGet<DeviceLogEntry[]>("/v1/settings/device-log");
}

export function listSerialPorts(): Promise<string[]> {
  return daemonGet<string[]>("/v1/device/serial-ports");
}

export async function forgetDevice(id: string): Promise<void> {
  await daemonPost("/v1/devices/forget", { id });
}

export function getWsPort(): Promise<number> {
  return getDaemonPort();
}

export function setPreferredDevice<T>(id: string): Promise<T[]> {
  return daemonPost<T[]>("/v1/devices/set-preferred", { id });
}

export function pairDevice<T>(id: string): Promise<T[]> {
  return daemonPost<T[]>("/v1/devices/pair", { id });
}

export function getCortexWsState<T>(): Promise<T> {
  return daemonInvoke<T>("get_cortex_ws_state");
}

export function getDeviceStatus<T>(): Promise<T> {
  return daemonGet<T>("/v1/status");
}

export function retryConnect<T>(): Promise<T> {
  return daemonPost<T>("/v1/control/retry-connect", {});
}

export function cancelRetry<T>(): Promise<T> {
  return daemonPost<T>("/v1/control/cancel-retry", {});
}
