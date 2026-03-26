<!-- SPDX-License-Identifier: GPL-3.0-only -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onMount } from "svelte";

type Totp = { id: string; name: string; created_at: number; revoked_at?: number | null; last_used_at?: number | null };
type Client = {
  id: string;
  name: string;
  endpoint_id: string;
  totp_id: string;
  scope: "read" | "full" | string;
  created_at: number;
  revoked_at?: number | null;
  last_connected_at?: number | null;
  last_ip?: string | null;
  last_country?: string | null;
  last_city?: string | null;
  last_locale?: string | null;
};

let port = 0;
let token = "";
let totp: Totp[] = [];
let clients: Client[] = [];
let endpointId = "";
let irohInfo: any = null;
let err = "";

let newTotpName = "";
let registerEndpoint = "";
let registerOtp = "";
let registerName = "";
let registerScope: "read" | "full" = "read";
let qr: string | null = null;

function fmt(ts?: number | null) {
  if (!ts) return "—";
  return new Date(ts * 1000).toLocaleString();
}

async function api(path: string, method = "GET", body?: any) {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (token) headers.Authorization = `Bearer ${token}`;
  const r = await fetch(`http://127.0.0.1:${port}${path}`, {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
  });
  const j = await r.json();
  if (!r.ok || j?.ok === false) throw new Error(j?.error || `HTTP ${r.status}`);
  return j;
}

async function refresh() {
  err = "";
  try {
    const [info, t, c] = await Promise.all([
      api("/v1/iroh/info"),
      api("/v1/iroh/totp"),
      api("/v1/iroh/clients"),
    ]);
    irohInfo = info;
    endpointId = info.endpoint_id || "";
    totp = t.totp || [];
    clients = c.clients || [];
  } catch (e: any) {
    err = String(e?.message || e);
  }
}

async function createTotp() {
  const r = await api("/v1/iroh/totp", "POST", { name: newTotpName || "Mobile" });
  qr = r.qr_png_base64;
  newTotpName = "";
  await refresh();
}
async function revokeTotp(id: string) { await api("/v1/iroh/totp/revoke", "POST", { id }); await refresh(); }
async function registerClient() {
  await api("/v1/iroh/clients/register", "POST", {
    endpoint_id: registerEndpoint,
    otp: registerOtp,
    name: registerName || undefined,
    scope: registerScope,
  });
  registerEndpoint = registerOtp = registerName = "";
  registerScope = "read";
  await refresh();
}
async function revokeClient(id: string) { await api("/v1/iroh/clients/revoke", "POST", { id }); await refresh(); }
async function setScope(id: string, scope: "read" | "full") { await api("/v1/iroh/clients/scope", "POST", { id, scope }); await refresh(); }

onMount(async () => {
  [port, token] = await Promise.all([
    invoke<number>("get_ws_port"),
    invoke<string>("get_api_token").catch(() => ""),
  ]);
  await refresh();
});
</script>

<div class="space-y-4">
  <div class="rounded-lg border p-3 text-xs">
    <div class="font-semibold mb-1">iroh Server</div>
    <div>Endpoint: <code class="break-all">{endpointId || "—"}</code></div>
    <div>Relay: <code class="break-all">{irohInfo?.relay_url || "—"}</code></div>
  </div>

  <div class="rounded-lg border p-3 text-xs space-y-2">
    <div class="font-semibold">Create TOTP (QR for phone)</div>
    <div class="flex gap-2">
      <input class="border rounded px-2 py-1 flex-1" bind:value={newTotpName} placeholder="Credential name" />
      <button class="border rounded px-2 py-1" onclick={createTotp}>Create</button>
    </div>
    {#if qr}
      <img src={qr} alt="TOTP QR" class="w-44 h-44 border rounded" />
    {/if}
  </div>

  <div class="rounded-lg border p-3 text-xs space-y-2">
    <div class="font-semibold">Authorize Client</div>
    <input class="border rounded px-2 py-1 w-full" bind:value={registerEndpoint} placeholder="Client iroh endpoint id" />
    <input class="border rounded px-2 py-1 w-full" bind:value={registerOtp} placeholder="Current OTP" />
    <input class="border rounded px-2 py-1 w-full" bind:value={registerName} placeholder="Client display name (optional)" />
    <div class="flex items-center gap-2">
      <label for="clients-register-scope">Scope</label>
      <select id="clients-register-scope" class="border rounded px-2 py-1" bind:value={registerScope}>
        <option value="read">read (default)</option>
        <option value="full">full ⚠️</option>
      </select>
      {#if registerScope === "full"}
        <span class="text-red-600 font-semibold">Warning: full grants complete control.</span>
      {/if}
    </div>
    <button class="border rounded px-2 py-1" onclick={registerClient}>Authorize</button>
  </div>

  <div class="rounded-lg border p-3 text-xs">
    <div class="font-semibold mb-2">TOTP Credentials</div>
    <div class="space-y-1">
      {#each totp as t}
        <div class="flex items-center gap-2 justify-between border rounded px-2 py-1">
          <div>{t.name} · created {fmt(t.created_at)} · revoked {fmt(t.revoked_at)} · last used {fmt(t.last_used_at)}</div>
          <button class="border rounded px-2 py-1" onclick={() => revokeTotp(t.id)}>Revoke</button>
        </div>
      {/each}
    </div>
  </div>

  <div class="rounded-lg border p-3 text-xs">
    <div class="font-semibold mb-2">Clients</div>
    <div class="space-y-1">
      {#each clients as c}
        <div class="border rounded px-2 py-1 space-y-1">
          <div class="flex justify-between items-center gap-2">
            <div>
              <div><b>{c.name}</b> · {c.scope}</div>
              <div class="break-all">{c.endpoint_id}</div>
              <div>last connect: {fmt(c.last_connected_at)} · {c.last_ip || "—"} {c.last_country || ""} {c.last_city || ""} {c.last_locale || ""}</div>
            </div>
            <div class="flex gap-1">
              <button class="border rounded px-2 py-1" onclick={() => setScope(c.id, "read")}>read</button>
              <button class="border rounded px-2 py-1 text-red-700" onclick={() => setScope(c.id, "full")}>full ⚠️</button>
              <button class="border rounded px-2 py-1" onclick={() => revokeClient(c.id)}>revoke</button>
            </div>
          </div>
        </div>
      {/each}
    </div>
  </div>

  {#if err}<div class="text-red-600 text-xs">{err}</div>{/if}
</div>
