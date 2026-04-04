<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- API Tokens — create, revoke, delete daemon API tokens with ACLs + expiration. -->
<script lang="ts">
import { onMount } from "svelte";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { t } from "$lib/i18n/index.svelte";

// ── Types ──────────────────────────────────────────────────────────────────

interface ApiToken {
  id: string;
  name: string;
  token: string;
  acl: string;
  created_at: number;
  expires_at: number | null;
  last_used_at: number | null;
  revoked: boolean;
}

type Acl = "admin" | "read_only" | "data" | "stream";
type Expiry = "week" | "month" | "quarter" | "never";

const ACL_OPTIONS: { key: Acl; label: string; desc: string }[] = [
  { key: "admin", label: "tokens.aclAdmin", desc: "tokens.aclAdminDesc" },
  { key: "read_only", label: "tokens.aclReadOnly", desc: "tokens.aclReadOnlyDesc" },
  { key: "data", label: "tokens.aclData", desc: "tokens.aclDataDesc" },
  { key: "stream", label: "tokens.aclStream", desc: "tokens.aclStreamDesc" },
];

const EXPIRY_OPTIONS: { key: Expiry; label: string }[] = [
  { key: "week", label: "tokens.expiryWeek" },
  { key: "month", label: "tokens.expiryMonth" },
  { key: "quarter", label: "tokens.expiryQuarter" },
  { key: "never", label: "tokens.expiryNever" },
];

// ── State ──────────────────────────────────────────────────────────────────

let tokens = $state<ApiToken[]>([]);
let loading = $state(true);
let creating = $state(false);
let newName = $state("");
let newAcl = $state<Acl>("admin");
let newExpiry = $state<Expiry>("month");
let justCreated = $state<ApiToken | null>(null);
let copied = $state(false);

// ── Actions ────────────────────────────────────────────────────────────────

async function refresh() {
  try {
    tokens = await daemonInvoke<ApiToken[]>("list_auth_tokens");
  } catch {
    tokens = [];
  }
  loading = false;
}

async function createToken() {
  if (!newName.trim()) return;
  creating = true;
  try {
    const token = await daemonInvoke<ApiToken>("create_auth_token", {
      name: newName.trim(),
      acl: newAcl,
      expiry: newExpiry,
    });
    justCreated = token;
    newName = "";
    await refresh();
  } catch (e) {
    console.error("create token error:", e);
  } finally {
    creating = false;
  }
}

async function revokeToken(id: string) {
  await daemonInvoke("revoke_auth_token", { id });
  await refresh();
}

async function deleteToken(id: string) {
  await daemonInvoke("delete_auth_token", { id });
  justCreated = null;
  await refresh();
}

async function copyToken(secret: string) {
  await navigator.clipboard.writeText(secret);
  copied = true;
  setTimeout(() => {
    copied = false;
  }, 2000);
}

function fmtDate(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

function fmtRelative(ts: number | null): string {
  if (!ts) return t("tokens.never");
  const diff = Date.now() / 1000 - ts;
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function statusBadge(tok: ApiToken): { label: string; color: string } {
  if (tok.revoked) return { label: t("tokens.revoked"), color: "bg-red-500/10 text-red-600 dark:text-red-400" };
  if (tok.expires_at && tok.expires_at * 1000 < Date.now())
    return { label: t("tokens.expired"), color: "bg-amber-500/10 text-amber-600 dark:text-amber-400" };
  return { label: t("tokens.active"), color: "bg-green-500/10 text-green-600 dark:text-green-400" };
}

onMount(refresh);
</script>

<div class="flex flex-col gap-4 px-1">

  <div class="flex flex-col gap-1">
    <h2 class="text-[0.72rem] font-bold tracking-tight text-foreground">{t("tokens.title")}</h2>
    <p class="text-[0.58rem] text-muted-foreground leading-relaxed">{t("tokens.desc")}</p>
  </div>

  <!-- Create form -->
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col gap-3 py-3">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">{t("tokens.create")}</span>

      <input type="text" bind:value={newName} placeholder={t("tokens.namePlaceholder")}
        class="h-7 rounded-md border border-border bg-background px-2 text-[0.68rem]
               text-foreground placeholder:text-muted-foreground/40
               focus:outline-none focus:ring-1 focus:ring-ring" />

      <div class="flex flex-col gap-1">
        <span class="text-[0.54rem] text-muted-foreground">{t("tokens.acl")}</span>
        <div class="flex gap-1.5 flex-wrap">
          {#each ACL_OPTIONS as opt}
            <button
              class="flex flex-col gap-0 px-2 py-1.5 rounded-md text-left cursor-pointer transition-colors
                     {newAcl === opt.key
                       ? 'bg-primary/10 border border-primary/30 text-foreground'
                       : 'bg-muted/30 border border-transparent text-muted-foreground hover:bg-muted/50'}"
              onclick={() => { newAcl = opt.key; }}>
              <span class="text-[0.58rem] font-medium">{t(opt.label)}</span>
              <span class="text-[0.48rem] text-muted-foreground/60">{t(opt.desc)}</span>
            </button>
          {/each}
        </div>
      </div>

      <div class="flex items-center gap-2">
        <span class="text-[0.54rem] text-muted-foreground">{t("tokens.expiry")}:</span>
        {#each EXPIRY_OPTIONS as opt}
          <button
            class="h-6 px-2 rounded-md text-[0.56rem] font-medium cursor-pointer transition-colors
                   {newExpiry === opt.key
                     ? 'bg-primary text-primary-foreground'
                     : 'bg-muted/40 text-muted-foreground hover:bg-muted'}"
            onclick={() => { newExpiry = opt.key; }}>
            {t(opt.label)}
          </button>
        {/each}
      </div>

      <Button size="sm" class="h-7 text-[0.58rem] self-start" disabled={!newName.trim() || creating}
        onclick={createToken}>
        {creating ? "…" : t("tokens.create")}
      </Button>
    </CardContent>
  </Card>

  <!-- Just-created banner -->
  {#if justCreated}
    <Card class="border-green-500/30 bg-green-500/5 gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col gap-2 py-3">
        <span class="text-[0.58rem] font-semibold text-green-700 dark:text-green-400">{t("tokens.copyWarning")}</span>
        <div class="flex items-center gap-2">
          <code class="flex-1 text-[0.62rem] font-mono bg-black/5 dark:bg-white/5 px-2 py-1 rounded select-all break-all">
            {justCreated.token}
          </code>
          <Button variant="outline" size="sm" class="h-7 text-[0.56rem] shrink-0"
            onclick={() => copyToken(justCreated!.token)}>
            {copied ? t("tokens.copied") : "Copy"}
          </Button>
        </div>
      </CardContent>
    </Card>
  {/if}

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- Token list -->
  {#if loading}
    <p class="text-[0.58rem] text-muted-foreground text-center py-6">Loading…</p>
  {:else if tokens.length === 0}
    <p class="text-[0.58rem] text-muted-foreground text-center py-6">{t("tokens.empty")}</p>
  {:else}
    <div class="flex flex-col gap-2">
      {#each tokens as tok (tok.id)}
        {@const badge = statusBadge(tok)}
        <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
          <CardContent class="flex items-start gap-3 py-3">
            <div class="flex flex-col gap-1 flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <span class="text-[0.65rem] font-semibold text-foreground truncate">{tok.name}</span>
                <span class="text-[0.46rem] font-bold tracking-wider uppercase px-1.5 py-0.5 rounded {badge.color}">
                  {badge.label}
                </span>
                <span class="text-[0.46rem] font-bold tracking-wider uppercase px-1.5 py-0.5 rounded bg-blue-500/10 text-blue-600 dark:text-blue-400">
                  {tok.acl}
                </span>
              </div>
              <div class="flex items-center gap-3 text-[0.5rem] text-muted-foreground/60">
                <span>{t("tokens.created")}: {fmtDate(tok.created_at)}</span>
                {#if tok.expires_at}
                  <span>{t("tokens.expiry")}: {fmtDate(tok.expires_at)}</span>
                {/if}
                <span>{t("tokens.lastUsed")}: {fmtRelative(tok.last_used_at)}</span>
              </div>
              <code class="text-[0.5rem] font-mono text-muted-foreground/40">{tok.token}</code>
            </div>
            <div class="flex items-center gap-1 shrink-0">
              {#if !tok.revoked}
                <Button variant="ghost" size="sm" class="h-6 text-[0.52rem] px-2 text-amber-600 hover:text-amber-700"
                  onclick={() => revokeToken(tok.id)}>
                  {t("tokens.revoke")}
                </Button>
              {/if}
              <Button variant="ghost" size="sm" class="h-6 text-[0.52rem] px-2 text-red-600 hover:text-red-700"
                onclick={() => deleteToken(tok.id)}>
                {t("tokens.delete")}
              </Button>
            </div>
          </CardContent>
        </Card>
      {/each}
    </div>
  {/if}
</div>
