// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Supported-devices catalog — **fetched from Rust at startup**.
 *
 * The single source of truth lives in `crates/skill-data/src/device.rs`
 * (`supported_companies()`).  This module provides types and a loader.
 */

import { invoke } from "@tauri-apps/api/core";

// ── Types (mirror Rust serde output) ──────────────────────────────────────────

export interface SupportedDeviceItem {
  name_key: string;
  image: string;
}

export interface SupportedCompany {
  id: string;
  name_key: string;
  devices: SupportedDeviceItem[];
  instruction_keys: string[];
}

export type SupportedCompanyId = string;

// ── Loader ────────────────────────────────────────────────────────────────────

/** Cached catalog (populated on first call). */
let _cache: SupportedCompany[] | null = null;

/**
 * Load the supported-companies catalog from Rust.
 * Returns a cached copy after the first successful call.
 */
export async function loadSupportedCompanies(): Promise<SupportedCompany[]> {
  if (_cache) return _cache;
  _cache = await invoke<SupportedCompany[]>("get_supported_companies");
  return _cache;
}

/**
 * Synchronous access to the cached catalog.
 * Returns `[]` if `loadSupportedCompanies()` hasn't completed yet.
 */
export function getSupportedCompanies(): SupportedCompany[] {
  return _cache ?? [];
}
