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

function timestamp(): string {
  return new Date().toISOString();
}

/* biome-ignore lint/suspicious/noConsole: this IS the logger */
const _debug = console.debug.bind(console);
/* biome-ignore lint/suspicious/noConsole: this IS the logger */
const _log = console.log.bind(console);
/* biome-ignore lint/suspicious/noConsole: this IS the logger */
const _warn = console.warn.bind(console);
/* biome-ignore lint/suspicious/noConsole: this IS the logger */
const _error = console.error.bind(console);

export const log = {
  /** Debug-level: stripped from production builds. */
  debug(...args: unknown[]): void {
    if (isDev) _debug(`[${timestamp()}] [DEBUG]`, ...args);
  },

  /** Informational: stripped from production builds. */
  info(...args: unknown[]): void {
    if (isDev) _log(`[${timestamp()}] [INFO]`, ...args);
  },

  /** Warning: preserved in production. */
  warn(...args: unknown[]): void {
    _warn(`[${timestamp()}] [WARN]`, ...args);
  },

  /** Error: preserved in production. */
  error(...args: unknown[]): void {
    _error(`[${timestamp()}] [ERROR]`, ...args);
  },
};
