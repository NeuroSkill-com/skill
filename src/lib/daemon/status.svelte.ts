// SPDX-License-Identifier: GPL-3.0-only
// Daemon connection status — reactive store for UI indicators.

export type DaemonConnectionState = "connected" | "connecting" | "disconnected" | "error";

interface DaemonStatus {
  state: DaemonConnectionState;
  version: string | null;
  lastError: string | null;
  lastConnectedAt: number | null;
  /** Round-trip latency to daemon in ms (null = unknown). */
  latencyMs: number | null;
}

// Svelte 5 reactive state
export const daemonStatus = $state<DaemonStatus>({
  state: "connecting",
  version: null,
  lastError: null,
  lastConnectedAt: null,
  latencyMs: null,
});

let _errorThrottle = 0;
let _prevState: DaemonConnectionState = "connecting";

/** Emit a toast when the daemon state changes meaningfully. */
function _notifyStateChange(next: DaemonConnectionState, error?: string): void {
  const prev = _prevState;
  _prevState = next;
  if (prev === next) return;

  // Skip the initial connecting→connected transition (normal startup)
  if (prev === "connecting" && next === "connected") return;

  Promise.all([import("$lib/stores/toast.svelte"), import("$lib/i18n/index.svelte")]).then(([{ addToast }, { t }]) => {
    if (next === "connected") {
      addToast("info", t("daemon.connection"), t("daemon.stateChangeConnected"), 4_000);
    } else if (next === "error") {
      addToast("error", t("daemon.connection"), error ?? t("daemon.stateError"), 6_000);
    } else if (next === "disconnected") {
      addToast("warning", t("daemon.connection"), t("daemon.stateChangeDisconnected"), 5_000);
    }
  });
}

export function setDaemonConnected(version?: string): void {
  _notifyStateChange("connected");
  daemonStatus.state = "connected";
  daemonStatus.version = version ?? daemonStatus.version;
  daemonStatus.lastError = null;
  daemonStatus.lastConnectedAt = Date.now();
}

export function setDaemonDisconnected(error?: string): void {
  const next = error ? "error" : "disconnected";
  _notifyStateChange(next, error);
  daemonStatus.state = next;
  daemonStatus.lastError = error ?? null;
  daemonStatus.latencyMs = null;
}

export function setDaemonConnecting(): void {
  _notifyStateChange("connecting");
  daemonStatus.state = "connecting";
}

export function setDaemonLatency(ms: number): void {
  daemonStatus.latencyMs = ms;
}

/**
 * Show a user-visible toast when the daemon is unreachable.
 * Throttled to max once per 30 seconds to avoid toast spam.
 */
export function notifyDaemonError(error: string): void {
  const now = Date.now();
  if (now - _errorThrottle < 30_000) return;
  _errorThrottle = now;

  setDaemonDisconnected(error);

  Promise.all([import("$lib/stores/toast.svelte"), import("$lib/i18n/index.svelte")]).then(([{ addToast }, { t }]) => {
    addToast("warning", t("daemon.connection"), t("daemon.unreachable", { error }), 8_000);
  });
}
