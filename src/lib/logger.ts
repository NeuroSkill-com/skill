// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Structured frontend logger.
//
// In production builds, console.log / console.debug are stripped by esbuild
// (see vite.config.js). This module provides a thin wrapper so call-sites
// are self-documenting and easy to grep.
//
// Usage:
//   import { log } from "$lib/logger";
//   log.info("device connected", { name: device.name });
//   log.warn("retrying BLE scan");
//   log.error("websocket closed", error);
//   log.debug("raw EEG sample", sample);  // stripped in production

const isDev = import.meta.env.DEV;

function _timestamp(): string {
  return new Date().toISOString();
}

export const log = {
  /** Debug-level: stripped from production builds. */
  debug(..._args: unknown[]): void {
    if (isDev)
  },

  /** Informational: stripped from production builds. */
  info(..._args: unknown[]): void {
    // biome-ignore lint/suspicious/noConsoleLog: structured logger
    if (isDev)
  },

  /** Warning: preserved in production. */
  warn(..._args: unknown[]): void {},

  /** Error: preserved in production. */
  error(..._args: unknown[]): void {},
};
