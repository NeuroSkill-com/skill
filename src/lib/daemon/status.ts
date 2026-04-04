// SPDX-License-Identifier: GPL-3.0-only
// Daemon connection status — reactive store for UI indicators.

export type DaemonConnectionState = "connected" | "connecting" | "disconnected" | "error";

interface DaemonStatus {
  state: DaemonConnectionState;
  version: string | null;
  lastError: string | null;
  lastConnectedAt: number | null;
}

// Svelte 5 reactive state
export const daemonStatus = $state<DaemonStatus>({
  state: "connecting",
  version: null,
  lastError: null,
  lastConnectedAt: null,
});

let _errorThrottle = 0;

export function setDaemonConnected(version?: string): void {
  daemonStatus.state = "connected";
  daemonStatus.version = version ?? daemonStatus.version;
  daemonStatus.lastError = null;
  daemonStatus.lastConnectedAt = Date.now();
}

export function setDaemonDisconnected(error?: string): void {
  daemonStatus.state = error ? "error" : "disconnected";
  daemonStatus.lastError = error ?? null;
}

export function setDaemonConnecting(): void {
  daemonStatus.state = "connecting";
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

  import("$lib/stores/toast.svelte").then(({ addToast }) => {
    addToast(
      "warning",
      "Daemon connection",
      `Unable to reach the daemon: ${error}. Some features may be unavailable.`,
      8_000,
    );
  });
}
