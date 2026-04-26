# Shared Daemon Client

Extracted the repeated fetch + auth token + timeout pattern into a shared `DaemonClient` class used by all features.

## Before

Every component (brain.ts, sidebar.ts, extension.ts) independently constructed fetch calls:
```typescript
const port = await discoverDaemonPort(config);
const base = `http://${config.daemonHost}:${port}/v1`;
const headers = { "Content-Type": "application/json" };
if (token) headers["Authorization"] = `Bearer ${token}`;
const resp = await fetch(`${base}${path}`, { method: "POST", headers, body, signal: AbortSignal.timeout(3000) });
```

## After

```typescript
const client = new DaemonClient(config, token);
const result = await client.post<FlowState>("/brain/flow-state", { windowSecs: 300 });
```

## Benefits

- Single place to update auth, timeout, port discovery
- All 8 new features use the shared client
- `setToken()` method for token refresh on reconnect
- Returns `null` on any failure (never throws) — all features handle gracefully

## Files

- `src/daemon-client.ts` — DaemonClient class (new)
