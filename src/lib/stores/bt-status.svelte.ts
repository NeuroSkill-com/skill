// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Centralised reactive store for Bluetooth availability status.
 *
 * Set by the main dashboard page when the BLE adapter state changes;
 * read by CustomTitleBar to tint the title bar red when BT is off.
 */

let btOff = $state(false);

/** Reactive getter — true when Bluetooth is unavailable. */
export function isBtOff(): boolean {
  return btOff;
}

/** Update the Bluetooth-off flag from the dashboard. */
export function setBtOff(off: boolean): void {
  btOff = off;
}
