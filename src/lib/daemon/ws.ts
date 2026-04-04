// SPDX-License-Identifier: GPL-3.0-only
// WebSocket client for daemon event stream (/v1/events).
// Replaces Tauri IPC Channels for EEG/PPG/IMU streaming + band power.

import { getDaemonPort } from "./http";

export interface DaemonEvent {
  type: string;
  ts_unix_ms: number;
  correlation_id?: string | null;
  payload: Record<string, unknown>;
}

export type EventHandler = (event: DaemonEvent) => void;

let _ws: WebSocket | null = null;
const _handlers = new Map<string, Set<EventHandler>>();
const _globalHandlers = new Set<EventHandler>();
let _reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let _token: string | null = null;

async function getWsUrl(): Promise<string> {
  const port = await getDaemonPort();
  return `ws://127.0.0.1:${port}/v1/events`;
}

/** Connect to the daemon WebSocket event stream. */
export async function connectDaemonWs(): Promise<void> {
  if (_ws && (_ws.readyState === WebSocket.OPEN || _ws.readyState === WebSocket.CONNECTING)) {
    return;
  }

  try {
    // Get token from bootstrap
    const { invoke } = await import("@tauri-apps/api/core");
    const bootstrap = await invoke<{ token: string }>("get_daemon_bootstrap");
    _token = bootstrap.token;
  } catch {
    _token = null;
  }

  const url = await getWsUrl();
  _ws = new WebSocket(url);

  _ws.onopen = () => {
    // Send auth token as first message if available
    if (_token && _ws) {
      _ws.send(JSON.stringify({ type: "auth", token: _token }));
    }
  };

  _ws.onmessage = (msg) => {
    try {
      const event: DaemonEvent = JSON.parse(msg.data);
      // Dispatch to type-specific handlers
      const handlers = _handlers.get(event.type);
      if (handlers) {
        for (const h of handlers) h(event);
      }
      // Dispatch to global handlers
      for (const h of _globalHandlers) h(event);
    } catch {
      // Ignore malformed messages
    }
  };

  _ws.onclose = () => {
    _ws = null;
    scheduleReconnect();
  };

  _ws.onerror = () => {
    _ws?.close();
  };
}

function scheduleReconnect() {
  if (_reconnectTimer) return;
  if (_handlers.size === 0 && _globalHandlers.size === 0) return;
  _reconnectTimer = setTimeout(() => {
    _reconnectTimer = null;
    connectDaemonWs().catch(() => {});
  }, 3000);
}

/** Subscribe to a specific event type. Returns an unsubscribe function. */
export function onDaemonEvent(type: string, handler: EventHandler): () => void {
  if (!_handlers.has(type)) _handlers.set(type, new Set());
  _handlers.get(type)!.add(handler);
  // Auto-connect on first subscription
  connectDaemonWs().catch(() => {});
  return () => {
    _handlers.get(type)?.delete(handler);
    if (_handlers.get(type)?.size === 0) _handlers.delete(type);
  };
}

/** Subscribe to all events. Returns an unsubscribe function. */
export function onAnyDaemonEvent(handler: EventHandler): () => void {
  _globalHandlers.add(handler);
  connectDaemonWs().catch(() => {});
  return () => {
    _globalHandlers.delete(handler);
  };
}

/** Disconnect the WebSocket. */
export function disconnectDaemonWs(): void {
  if (_reconnectTimer) {
    clearTimeout(_reconnectTimer);
    _reconnectTimer = null;
  }
  _ws?.close();
  _ws = null;
}

/** Check if the WebSocket is connected. */
export function isDaemonWsConnected(): boolean {
  return _ws?.readyState === WebSocket.OPEN;
}
