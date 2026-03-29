<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Clients tab — pair phones via QR, manage connected devices and their permissions. -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onDestroy, onMount } from "svelte";
import { Badge } from "$lib/components/ui/badge";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";

type Totp = { id: string; name: string; created_at: number; revoked_at?: number | null; last_used_at?: number | null };
type Permissions = {
  scope: string;
  groups: string[];
  allow: string[];
  deny: string[];
};
type Client = {
  id: string;
  name: string;
  endpoint_id: string;
  totp_id: string;
  scope: string;
  permissions: Permissions;
  created_at: number;
  revoked_at?: number | null;
  last_connected_at?: number | null;
  last_ip?: string | null;
  last_country?: string | null;
  last_city?: string | null;
  last_locale?: string | null;
};
type CommandGroup = {
  id: string;
  label: string;
  description: string;
  dangerous: boolean;
  commands: string[];
};

let port = $state(0);
let token = $state("");
let totp = $state<Totp[]>([]);
let clients = $state<Client[]>([]);
let irohInfo = $state<any>(null);
let err = $state("");
let loading = $state(true);

// QR flow
let qr = $state<string | null>(null);
let inviteLink = $state<string | null>(null);
let linkCopied = $state(false);
let creating = $state(false);
let showSuccess = $state(false);
let inviteScope = $state<"read" | "custom" | "full">("read");

// Permissions editor
let editingClientId = $state<string | null>(null);
let scopeGroups = $state<CommandGroup[]>([]);
let editScope = $state<string>("read");
let editGroups = $state<Set<string>>(new Set());
let editAllow = $state<string[]>([]);
let editDeny = $state<string[]>([]);
let saving = $state(false);

let online = $derived(!!irohInfo?.online);
let activeTotp = $derived(totp.filter((t) => !t.revoked_at));
let activeClients = $derived(clients.filter((c) => !c.revoked_at));
let revokedClients = $derived(clients.filter((c) => c.revoked_at));
let revokedTotp = $derived(totp.filter((t) => t.revoked_at));
let safeGroups = $derived(scopeGroups.filter((g) => !g.dangerous));
let dangerousGroups = $derived(scopeGroups.filter((g) => g.dangerous));
let editingClient = $derived(editingClientId ? activeClients.find((c) => c.id === editingClientId) : null);
let editHasDangerous = $derived(
  editScope === "full" || (editScope === "custom" && dangerousGroups.some((g) => editGroups.has(g.id))),
);

// Merged view: each "device" is a client, enriched with its TOTP info
let devices = $derived(
  activeClients.map((c) => {
    const t = activeTotp.find((t) => t.id === c.totp_id);
    return { ...c, totp_name: t?.name ?? "", totp_created: t?.created_at ?? 0 };
  }),
);

let revokedCount = $derived(revokedClients.length + revokedTotp.length);

function fmt(ts?: number | null) {
  if (!ts) return "—";
  return new Date(ts * 1000).toLocaleString();
}

function ago(ts?: number | null) {
  if (!ts) return "";
  const s = Math.floor(Date.now() / 1000 - ts);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
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
    const [info, t, c] = await Promise.all([api("/v1/iroh/info"), api("/v1/iroh/totp"), api("/v1/iroh/clients")]);
    irohInfo = info;
    totp = t.totp || [];
    clients = c.clients || [];
  } catch (e: any) {
    err = String(e?.message || e);
  } finally {
    loading = false;
  }
}

async function loadScopeGroups() {
  try {
    const r = await api("/v1/iroh/scope-groups");
    scopeGroups = r.groups || [];
  } catch {
    /* ignore */
  }
}

// ── QR invite ──────────────────────────────────────────────────────────────
let clientCountBeforeQr = 0;
let pollTimer: ReturnType<typeof setInterval> | null = null;

async function createInvite() {
  err = "";
  creating = true;
  showSuccess = false;
  linkCopied = false;
  try {
    clientCountBeforeQr = activeClients.length;
    const r = await api("/v1/iroh/phone-invite", "POST", {
      name: "Invite",
      scope: inviteScope,
    });
    qr = r.qr_png_base64;
    // Build a deep link from the invite payload so users can copy/paste
    // it when camera access is unavailable (e.g. simulator, no camera).
    if (r.payload) {
      const json = JSON.stringify(r.payload);
      const b64 = btoa(json).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
      inviteLink = `neuroskill://invite/${b64}`;
    }
    await refresh();
    startPolling();
  } catch (e: any) {
    err = String(e?.message || e);
  } finally {
    creating = false;
  }
}

async function copyInviteLink() {
  if (!inviteLink) return;
  try {
    await navigator.clipboard.writeText(inviteLink);
    linkCopied = true;
    setTimeout(() => (linkCopied = false), 2000);
  } catch {
    // Fallback for environments where clipboard API is unavailable
    const ta = document.createElement("textarea");
    ta.value = inviteLink;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    document.body.removeChild(ta);
    linkCopied = true;
    setTimeout(() => (linkCopied = false), 2000);
  }
}

function startPolling() {
  stopPolling();
  pollTimer = setInterval(async () => {
    await refresh();
    if (activeClients.length > clientCountBeforeQr) {
      showSuccess = true;
      qr = null;
      inviteLink = null;
      linkCopied = false;
      stopPolling();
      setTimeout(() => {
        showSuccess = false;
      }, 4000);
    }
  }, 2000);
  setTimeout(() => stopPolling(), 5 * 60 * 1000);
}

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

// ── Device actions ─────────────────────────────────────────────────────────
async function revokeDevice(clientId: string) {
  try {
    await api("/v1/iroh/clients/revoke", "POST", { id: clientId });
  } catch (e: any) {
    err = String(e?.message || e);
  }
  editingClientId = null;
  await refresh();
}

function openPermissions(c: Client) {
  editingClientId = c.id;
  editScope = c.permissions?.scope || c.scope || "read";
  editGroups = new Set(c.permissions?.groups || []);
  editAllow = [...(c.permissions?.allow || [])];
  editDeny = [...(c.permissions?.deny || [])];
}

function closePermissions() {
  editingClientId = null;
}

function toggleGroup(gid: string) {
  if (editGroups.has(gid)) editGroups.delete(gid);
  else editGroups.add(gid);
  editGroups = new Set(editGroups);
}

function setPresetScope(scope: "read" | "full" | "custom") {
  editScope = scope;
  if (scope !== "custom") {
    editGroups = new Set();
    editAllow = [];
    editDeny = [];
  }
}

async function savePermissions() {
  if (!editingClientId) return;
  saving = true;
  err = "";
  try {
    await api("/v1/iroh/clients/scope", "POST", {
      id: editingClientId,
      scope: editScope,
      groups: editScope === "custom" ? [...editGroups] : undefined,
      allow: editScope === "custom" && editAllow.length ? editAllow : undefined,
      deny: editScope === "custom" && editDeny.length ? editDeny : undefined,
    });
    editingClientId = null;
    await refresh();
  } catch (e: any) {
    err = String(e?.message || e);
  } finally {
    saving = false;
  }
}

onMount(async () => {
  [port, token] = await Promise.all([invoke<number>("get_ws_port"), invoke<string>("get_api_token").catch(() => "")]);
  await Promise.all([refresh(), loadScopeGroups()]);
});

onDestroy(() => stopPolling());
</script>

<!-- ── Permissions Editor Modal ─────────────────────────────────────────────── -->
{#if editingClient}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 bg-black/50 flex items-start justify-center pt-8 overflow-y-auto"
    onclick={(e) => {
      if (e.target === e.currentTarget) closePermissions();
    }}
  >
    <div
      class="bg-white dark:bg-[#14141e] rounded-xl border border-border dark:border-white/[0.06] shadow-2xl w-full max-w-lg mx-4 mb-8"
    >
      <div class="p-4 border-b border-border dark:border-white/[0.05]">
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-sm font-semibold">Permissions — {editingClient.name}</h3>
            <p class="text-[0.52rem] text-muted-foreground font-mono mt-0.5">{editingClient.endpoint_id}</p>
          </div>
          <Button variant="ghost" size="sm" class="h-7 w-7 p-0" onclick={closePermissions}>
            <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path d="M6 18L18 6M6 6l12 12" />
            </svg>
          </Button>
        </div>
      </div>

      <div class="p-4 flex flex-col gap-4 max-h-[70vh] overflow-y-auto">
        <!-- Scope presets -->
        <div>
          <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">Scope</span>
          <div class="flex gap-2 mt-1.5">
            {#each [["read", "Read Only", ""], ["custom", "Custom", ""], ["full", "Full Access", "text-red-600"]] as [val, label, cls]}
              <button
                class="px-3 py-1.5 text-xs rounded-md border transition-colors
                       {editScope === val
                  ? val === 'full'
                    ? 'bg-red-600 text-white border-red-600'
                    : 'bg-primary text-primary-foreground border-primary'
                  : `border-border dark:border-white/[0.1] hover:bg-muted ${cls}`}"
                onclick={() => setPresetScope(val as "read" | "custom" | "full")}
              >
                {label}
              </button>
            {/each}
          </div>
        </div>

        {#if editScope === "full"}
          <div class="rounded-lg border-2 border-red-300 dark:border-red-700/60 bg-red-50 dark:bg-red-900/15 p-3">
            <div class="flex items-center gap-2 mb-1.5">
              <svg
                class="h-4 w-4 text-red-500 shrink-0"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
                />
              </svg>
              <span class="text-xs font-bold text-red-700 dark:text-red-300">Full Access Warning</span>
            </div>
            <p class="text-[0.6rem] text-red-600 dark:text-red-400 leading-relaxed">
              Complete control: manage hooks, LLM, credentials, other clients' permissions, and system settings.
            </p>
          </div>
        {/if}

        {#if editScope === "custom"}
          <div>
            <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground"
              >Command Groups</span
            >
            <div class="flex flex-col gap-1 mt-1.5">
              {#each safeGroups as g}
                <label
                  class="flex items-start gap-2.5 px-2.5 py-2 rounded-lg border border-border dark:border-white/[0.06]
                              hover:bg-muted/50 cursor-pointer transition-colors"
                >
                  <input
                    type="checkbox"
                    checked={editGroups.has(g.id)}
                    onchange={() => toggleGroup(g.id)}
                    class="mt-0.5 rounded accent-primary"
                  />
                  <div class="flex-1 min-w-0">
                    <div class="text-xs font-medium">{g.label}</div>
                    <div class="text-[0.56rem] text-muted-foreground leading-relaxed">{g.description}</div>
                  </div>
                </label>
              {/each}
            </div>
          </div>

          {#if dangerousGroups.length > 0}
            <div>
              <div class="flex items-center gap-2 mb-1.5">
                <svg
                  class="h-4 w-4 text-red-500"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  stroke-width="2"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
                  />
                </svg>
                <span
                  class="text-[0.56rem] font-semibold tracking-widest uppercase text-red-600 dark:text-red-400"
                >
                  Dangerous
                </span>
              </div>
              {#if editHasDangerous}
                <div
                  class="rounded-lg border-2 border-red-300 dark:border-red-700/60 bg-red-50 dark:bg-red-900/15 p-2.5 mb-2"
                >
                  <p class="text-[0.56rem] text-red-600 dark:text-red-400 leading-relaxed">
                    These groups grant system-level control. Only enable for fully trusted devices.
                  </p>
                </div>
              {/if}
              <div class="flex flex-col gap-1">
                {#each dangerousGroups as g}
                  <label
                    class="flex items-start gap-2.5 px-2.5 py-2 rounded-lg border-2 transition-colors cursor-pointer
                                {editGroups.has(g.id)
                      ? 'border-red-300 dark:border-red-700/60 bg-red-50/50 dark:bg-red-900/10'
                      : 'border-border dark:border-white/[0.06] hover:bg-muted/50'}"
                  >
                    <input
                      type="checkbox"
                      checked={editGroups.has(g.id)}
                      onchange={() => toggleGroup(g.id)}
                      class="mt-0.5 rounded accent-red-500"
                    />
                    <div class="flex-1 min-w-0">
                      <div class="flex items-center gap-1.5">
                        <span class="text-xs font-medium">{g.label}</span>
                        <Badge variant="destructive" class="text-[0.45rem] px-1 py-0">dangerous</Badge>
                      </div>
                      <div class="text-[0.56rem] text-muted-foreground leading-relaxed">{g.description}</div>
                    </div>
                  </label>
                {/each}
              </div>
            </div>
          {/if}
        {/if}

        {#if editScope === "read"}
          <div class="text-[0.6rem] text-muted-foreground bg-muted/40 rounded-lg p-3 leading-relaxed">
            <strong>Read-only</strong> — Can view status, search data, browse screenshots, and check health data.
            Cannot create labels, control the device, or modify settings.
          </div>
        {/if}
      </div>

      <div class="p-4 border-t border-border dark:border-white/[0.05] flex items-center justify-between">
        <Button
          variant="outline"
          size="sm"
          class="text-xs text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20"
          onclick={() => revokeDevice(editingClient!.id)}
        >
          Revoke Device
        </Button>
        <div class="flex gap-2">
          <Button variant="outline" size="sm" class="text-xs" onclick={closePermissions}>Cancel</Button>
          <Button size="sm" class="text-xs" disabled={saving} onclick={savePermissions}>
            {saving ? "Saving…" : "Save"}
          </Button>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- ── Status ──────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    Remote Access
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="p-4">
      {#if loading}
        <p class="text-xs text-muted-foreground">Loading…</p>
      {:else if !online}
        <div class="flex items-center gap-2">
          <span class="h-2 w-2 rounded-full bg-red-400 animate-pulse"></span>
          <span class="text-xs text-muted-foreground">iroh tunnel is offline</span>
        </div>
      {:else}
        <div class="flex items-center gap-2">
          <span class="h-2 w-2 rounded-full bg-emerald-400"></span>
          <span class="text-xs font-medium">Online</span>
          <Badge variant="secondary" class="text-[0.5rem]">
            {devices.length} device{devices.length !== 1 ? "s" : ""}
          </Badge>
        </div>
        <p class="mt-2 text-[0.56rem] text-muted-foreground font-mono break-all leading-relaxed">
          {irohInfo.endpoint_id}
        </p>
      {/if}
    </CardContent>
  </Card>
</section>

{#if online}
  <!-- ── Pair a Device ──────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      Pair a Device
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="p-4 flex flex-col gap-3">
        {#if showSuccess}
          <div class="flex flex-col items-center gap-2 py-4 animate-in fade-in duration-500">
            <div
              class="h-16 w-16 rounded-full bg-emerald-100 dark:bg-emerald-900/30 flex items-center justify-center"
            >
              <svg
                class="h-8 w-8 text-emerald-500"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="3"
              >
                <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
              </svg>
            </div>
            <p class="text-sm font-semibold text-emerald-600 dark:text-emerald-400">Device paired</p>
          </div>
        {:else if qr}
          <div class="flex flex-col items-center gap-3">
            <img src={qr} alt="Invite QR" class="w-48 h-48 rounded-lg border" />
            <p class="text-[0.6rem] text-muted-foreground text-center max-w-56">
              Scan with the Skill mobile app. The device connects automatically.
            </p>

            <!-- Copy invite link (for simulator / no-camera devices) -->
            {#if inviteLink}
              <div class="flex flex-col items-center gap-1.5 w-full max-w-64">
                <button
                  onclick={copyInviteLink}
                  class="flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-border dark:border-white/[0.1]
                         text-[0.58rem] text-muted-foreground hover:text-foreground hover:bg-muted
                         transition-colors cursor-pointer w-full justify-center"
                >
                  {#if linkCopied}
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
                         stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 text-emerald-500 shrink-0">
                      <polyline points="20 6 9 17 4 12"/>
                    </svg>
                    <span class="text-emerald-600 dark:text-emerald-400 font-medium">Copied!</span>
                  {:else}
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                         stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 shrink-0">
                      <rect x="9" y="9" width="13" height="13" rx="2" ry="2"/>
                      <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/>
                    </svg>
                    <span>Copy invite link</span>
                  {/if}
                </button>
                <p class="text-[0.48rem] text-muted-foreground/60 text-center leading-tight">
                  Open this link on the phone if you can't scan the QR code
                </p>
              </div>
            {/if}

            <div class="flex items-center gap-2">
              <Badge
                variant={inviteScope === "full" ? "destructive" : inviteScope === "custom" ? "default" : "secondary"}
                class="text-[0.5rem]"
              >
                {inviteScope === "read" ? "Read Only" : inviteScope === "full" ? "Full Access" : "Custom"}
              </Badge>
              <Button
                variant="outline"
                size="sm"
                class="text-xs"
                onclick={() => {
                  qr = null;
                  inviteLink = null;
                  linkCopied = false;
                  stopPolling();
                }}
              >
                Cancel
              </Button>
            </div>
          </div>
        {:else}
          <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
            Generate a QR code for the Skill mobile app. It contains the server address, relay, and a one-time credential.
          </p>

          <!-- Scope picker -->
          <div>
            <span class="text-[0.52rem] font-semibold tracking-widest uppercase text-muted-foreground">
              Permissions
            </span>
            <div class="flex gap-2 mt-1.5">
              {#each [["read", "Read Only"], ["custom", "Custom"], ["full", "Full Access"]] as [val, label]}
                <button
                  class="px-3 py-1.5 text-[0.62rem] rounded-md border transition-colors cursor-pointer
                         {inviteScope === val
                    ? val === 'full'
                      ? 'bg-red-600 text-white border-red-600'
                      : 'bg-primary text-primary-foreground border-primary'
                    : `border-border dark:border-white/[0.1] hover:bg-muted ${val === 'full' ? 'text-red-600' : ''}`}"
                  onclick={() => (inviteScope = val as "read" | "custom" | "full")}
                >
                  {label}
                </button>
              {/each}
            </div>
            {#if inviteScope === "read"}
              <p class="text-[0.52rem] text-muted-foreground mt-1.5">
                View status, search data, browse screenshots. Cannot control the device or change settings.
              </p>
            {:else if inviteScope === "full"}
              <p class="text-[0.52rem] text-red-600 dark:text-red-400 mt-1.5">
                Full control over the device. Only for trusted devices.
              </p>
            {:else}
              <p class="text-[0.52rem] text-muted-foreground mt-1.5">
                Custom permissions can be configured after pairing.
              </p>
            {/if}
          </div>

          <Button size="sm" class="text-xs self-start" disabled={creating} onclick={createInvite}>
            {creating ? "Generating…" : "Generate QR Code"}
          </Button>
        {/if}
      </CardContent>
    </Card>
  </section>

  <!-- ── Paired Devices ─────────────────────────────────────────────────── -->
  {#if devices.length > 0}
    <section class="flex flex-col gap-2">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
        Paired Devices ({devices.length})
      </span>
      <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
        <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">
          {#each devices as d (d.id)}
            <div class="flex items-start justify-between gap-3 px-4 py-3">
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-1.5 flex-wrap">
                  <span class="text-[0.72rem] font-semibold truncate">{d.name}</span>
                  <Badge
                    variant={d.scope === "full" ? "destructive" : d.scope === "custom" ? "default" : "secondary"}
                    class="text-[0.48rem]"
                  >
                    {d.scope === "read" ? "Read Only" : d.scope === "full" ? "Full Access" : "Custom"}
                  </Badge>
                  {#if d.last_connected_at}
                    <span class="text-[0.5rem] text-muted-foreground">{ago(d.last_connected_at)}</span>
                  {/if}
                </div>
                <div class="text-[0.52rem] text-muted-foreground mt-0.5 font-mono truncate">
                  {d.endpoint_id}
                </div>
                {#if d.last_ip || d.last_country || d.last_city}
                  <div class="text-[0.5rem] text-muted-foreground/60 mt-0.5">
                    {[d.last_city, d.last_country, d.last_ip].filter(Boolean).join(" · ")}
                  </div>
                {/if}
              </div>
              <div class="flex gap-1 shrink-0 pt-0.5">
                <Button
                  variant="outline"
                  size="sm"
                  class="text-[0.54rem] h-6 px-2"
                  onclick={() => openPermissions(d)}
                >
                  Permissions
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  class="text-[0.54rem] h-6 px-2 text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20"
                  onclick={() => revokeDevice(d.id)}
                >
                  Revoke
                </Button>
              </div>
            </div>
          {/each}
        </CardContent>
      </Card>
    </section>
  {/if}

  <!-- ── Revoked ────────────────────────────────────────────────────────── -->
  {#if revokedCount > 0}
    <section class="flex flex-col gap-2">
      <details class="group">
        <summary
          class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5 cursor-pointer select-none
                        list-none flex items-center gap-1"
        >
          <svg
            class="h-3 w-3 transition-transform group-open:rotate-90"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M9 5l7 7-7 7" />
          </svg>
          Revoked ({revokedCount})
        </summary>
        <Card
          class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden mt-2 opacity-60"
        >
          <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">
            {#each revokedClients as c}
              <div class="px-4 py-2">
                <div class="flex items-center gap-1.5">
                  <span class="text-[0.6rem] line-through">{c.name}</span>
                  <span class="text-[0.5rem] text-muted-foreground">revoked {fmt(c.revoked_at)}</span>
                </div>
              </div>
            {/each}
          </CardContent>
        </Card>
      </details>
    </section>
  {/if}
{/if}

{#if err}
  <div
    class="rounded-lg border border-red-200 dark:border-red-800/40 bg-red-50 dark:bg-red-900/10 px-3 py-2 text-xs text-red-700 dark:text-red-400"
  >
    {err}
  </div>
{/if}
