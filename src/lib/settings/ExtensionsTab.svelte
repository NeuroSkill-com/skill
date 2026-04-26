<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "$lib/i18n/index.svelte";
import { SectionHeader } from "$lib/components/ui/section-header";
import { SettingsCard } from "$lib/components/ui/settings-card";
import { CardContent } from "$lib/components/ui/card";
import { Button } from "$lib/components/ui/button";
import { Badge } from "$lib/components/ui/badge";

// ── Types ────────────────────────────────────────────────────────────────────

interface ExtensionInfo {
  id: string;
  nameKey: string;
  descKey: string;
  icon: string;
  installed: boolean;
  installing: boolean;
  storeUrl: string;
  canAutoInstall: boolean;
}

// ── State ────────────────────────────────────────────────────────────────────

// VS Code-family forks: one row per available fork (detected at runtime).
// `id` matches the backend fork id (vscode, vscode-insiders, vscodium, cursor, …).
interface VsForkUI {
  id: string;
  name: string;
  available: boolean;
  installed: boolean;
  installing: boolean;
}
let vsForks = $state<VsForkUI[]>([]);

let extensions = $state<ExtensionInfo[]>([
  {
    id: "chrome",
    nameKey: "extensions.chrome",
    descKey: "extensions.chromeDesc",
    icon: `<circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="4"/><line x1="21.17" y1="8" x2="12" y2="8"/><line x1="3.95" y1="6.06" x2="8.54" y2="14"/><line x1="10.88" y1="21.94" x2="15.46" y2="14"/>`,
    installed: false,
    installing: false,
    storeUrl: "",
    canAutoInstall: true,
  },
  {
    id: "firefox",
    nameKey: "extensions.firefox",
    descKey: "extensions.firefoxDesc",
    icon: `<circle cx="12" cy="12" r="10"/><path d="M12 2a7 7 0 0 0-7 7c0 3.87 3.13 7 7 7s7-3.13 7-7a7 7 0 0 0-7-7z"/>`,
    installed: false,
    installing: false,
    storeUrl: "",
    canAutoInstall: true,
  },
  {
    id: "edge",
    nameKey: "extensions.edge",
    descKey: "extensions.edgeDesc",
    icon: `<circle cx="12" cy="12" r="10"/><path d="M2 12 12 12 22 12"/><path d="M12 2 12 22"/>`,
    installed: false,
    installing: false,
    storeUrl: "",
    canAutoInstall: true,
  },
  {
    id: "safari",
    nameKey: "extensions.safari",
    descKey: "extensions.safariDesc",
    icon: `<circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>`,
    installed: false,
    installing: false,
    storeUrl: "",
    canAutoInstall: true,
  },
]);

// Per-extension message (so each row shows its own success/error)
let extMessage = $state<Record<string, { text: string; type: "success" | "error" }>>({});

let statusMessage = $state("");
let statusType = $state<"success" | "error" | "">("");

// ── Actions ──────────────────────────────────────────────────────────────────

async function installExtension(ext: ExtensionInfo): Promise<void> {
  const idx = extensions.findIndex((e) => e.id === ext.id);
  if (idx < 0) return;
  extensions[idx].installing = true;
  extMessage[ext.id] = { text: "Installing...", type: "success" };

  try {
    const result = await invoke<{ ok: boolean; message: string }>("install_extension", {
      extensionId: ext.id,
    });
    if (result.ok) {
      extensions[idx].installed = true;
      extMessage[ext.id] = { text: result.message, type: "success" };
    } else {
      extMessage[ext.id] = { text: result.message, type: "error" };
    }
  } catch (e: any) {
    extMessage[ext.id] = { text: e.message ?? String(e), type: "error" };
  }
  extensions[idx].installing = false;
  // Auto-clear after 8 seconds
  setTimeout(() => { delete extMessage[ext.id]; extMessage = { ...extMessage }; }, 8000);
}

async function openStore(ext: ExtensionInfo): Promise<void> {
  if (ext.storeUrl) {
    await openUrl(ext.storeUrl);
  }
}

async function checkInstalled(): Promise<void> {
  try {
    const result = await invoke<{
      vscode?: boolean;
      vscode_forks?: Array<{ id: string; name: string; available: boolean; installed: boolean }>;
    } & Record<string, boolean>>("check_extensions_installed");

    // VS Code forks: keep only ones the user actually has installed (available),
    // so the UI doesn't list 7 editors for someone who runs only one.
    if (Array.isArray(result.vscode_forks)) {
      const next: VsForkUI[] = result.vscode_forks
        .filter((f) => f.available)
        .map((f) => {
          const prev = vsForks.find((p) => p.id === f.id);
          return {
            id: f.id,
            name: f.name,
            available: f.available,
            installed: f.installed,
            installing: prev?.installing ?? false,
          };
        });
      vsForks = next;
    }

    // Browser extensions
    for (let i = 0; i < extensions.length; i++) {
      const v = (result as Record<string, boolean>)[extensions[i].id];
      if (v !== undefined) extensions[i].installed = v;
    }
  } catch (e) {
    console.error("checkInstalled failed:", e);
  }
}

async function installFork(fork: VsForkUI): Promise<void> {
  const idx = vsForks.findIndex((f) => f.id === fork.id);
  if (idx < 0) return;
  vsForks[idx].installing = true;
  extMessage[fork.id] = { text: t("extensions.installing"), type: "success" };
  try {
    const result = await invoke<{ ok: boolean; message: string }>("install_extension", {
      extensionId: fork.id,
    });
    if (result.ok) {
      vsForks[idx].installed = true;
      extMessage[fork.id] = { text: result.message, type: "success" };
    } else {
      extMessage[fork.id] = { text: result.message, type: "error" };
    }
  } catch (e: any) {
    extMessage[fork.id] = { text: e.message ?? String(e), type: "error" };
    console.error("installFork failed:", e);
  }
  vsForks[idx].installing = false;
  setTimeout(() => { delete extMessage[fork.id]; extMessage = { ...extMessage }; }, 8000);
}

async function copyAuthToken(): Promise<void> {
  try {
    const { getDaemonBaseUrl } = await import("$lib/daemon/http");
    const { token } = await getDaemonBaseUrl();
    await navigator.clipboard.writeText(token);
    statusMessage = t("extensions.tokenCopied");
    statusType = "success";
  } catch {
    statusMessage = t("extensions.tokenFailed");
    statusType = "error";
  }
  setTimeout(() => { statusMessage = ""; }, 3000);
}

/**
 * Generate a one-time pairing token and copy it to the clipboard with a
 * recognizable prefix. The browser extension popup auto-detects this prefix
 * and redeems the token automatically — zero typing, zero deep links.
 */
async function copyPairingToken(): Promise<void> {
  try {
    const { daemonPost } = await import("$lib/daemon/http");
    const result = await daemonPost<{ pin: string; token: string; expires_in_secs: number }>("/v1/pair/start");
    // Prefix lets the extension popup recognize it (and ignore unrelated clipboard data)
    const payload = `neuroskill-pair:${result.token}`;
    await navigator.clipboard.writeText(payload);
    statusMessage = t("extensions.clipboardPairCopied");
    statusType = "success";
  } catch {
    statusMessage = t("extensions.tokenFailed");
    statusType = "error";
  }
  setTimeout(() => { statusMessage = ""; }, 5000);
}

let pairingInProgress = $state(false);

async function pairViaBrowser(): Promise<void> {
  pairingInProgress = true;
  statusMessage = "";
  try {
    const { daemonPost } = await import("$lib/daemon/http");
    const result = await daemonPost<{ code: string; url: string }>("/v1/pair/generate-code");
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(result.url);
    statusMessage = t("extensions.pairingOpened");
    statusType = "success";
  } catch {
    statusMessage = t("extensions.pairingFailed");
    statusType = "error";
  }
  pairingInProgress = false;
  setTimeout(() => { statusMessage = ""; }, 5000);
}

/** Enable Safari's Develop menu and try to toggle "Allow Unsigned Extensions". */
let enablingUnsigned = $state(false);
async function enableSafariUnsignedExtensions(): Promise<void> {
  enablingUnsigned = true;
  extMessage["safari"] = { text: "Enabling Develop menu…", type: "success" };
  try {
    const result = await invoke<{ ok: boolean; message: string; needs_accessibility?: boolean; auto_clicked?: boolean }>(
      "enable_safari_unsigned_extensions",
    );
    extMessage["safari"] = {
      text: result.message,
      type: result.ok ? "success" : "error",
    };
  } catch (e: any) {
    extMessage["safari"] = { text: e.message ?? String(e), type: "error" };
  }
  enablingUnsigned = false;
  setTimeout(() => { delete extMessage["safari"]; extMessage = { ...extMessage }; }, 12000);
}

// Check status on mount
$effect(() => {
  checkInstalled();
});
</script>

<div class="flex flex-col gap-5">

  <!-- IDE Extensions: one row per detected VS Code-family editor -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("extensions.ideTitle")}</SectionHeader>
    <SettingsCard>
      <CardContent class="py-0 px-0">
        <div class="divide-y divide-border dark:divide-white/[0.04]">
          {#if vsForks.length === 0}
            <p class="px-4 py-3 text-sm text-muted-foreground">
              {t("extensions.noIdeDetected")}
            </p>
          {/if}
          {#each vsForks as fork}
            <div class="flex items-center justify-between gap-3 px-4 py-3.5">
              <div class="flex min-w-0 flex-1 items-center gap-3">
                <div class="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md bg-muted dark:bg-white/[0.06]">
                  <svg viewBox="0 0 24 24" class="h-5 w-5" fill="currentColor" stroke="none">
                    <path d="M17.583 2.237L10.82 8.363 5.94 4.657 3.5 5.726v12.548l2.44 1.069 4.88-3.706 6.763 6.126L20.5 19.92V4.08l-2.917-1.843zM10 12l-4.146 3.291V8.709L10 12zm7.5 4.846L13.038 12l4.462-4.846v9.692z"/>
                  </svg>
                </div>
                <div class="min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="text-sm font-medium">{fork.name}</span>
                    {#if fork.installed}
                      <Badge variant="default">{t("extensions.installed")}</Badge>
                    {/if}
                  </div>
                  {#if extMessage[fork.id]}
                    <p class="mt-1 text-xs {extMessage[fork.id].type === 'success' ? 'text-green-500' : 'text-red-500'}">
                      {extMessage[fork.id].text}
                    </p>
                  {/if}
                </div>
              </div>
              <div class="flex flex-shrink-0 items-center gap-2">
                <Button
                  size="sm"
                  variant={fork.installed ? "outline" : "default"}
                  disabled={fork.installing}
                  onclick={() => installFork(fork)}
                >
                  {#if fork.installing}
                    {t("extensions.installing")}
                  {:else if fork.installed}
                    {t("extensions.reinstall")}
                  {:else}
                    {t("extensions.install")}
                  {/if}
                </Button>
              </div>
            </div>
          {/each}
        </div>
      </CardContent>
    </SettingsCard>
  </section>

  <!-- Browser Extensions -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("extensions.browserTitle")}</SectionHeader>
    <p class="text-sm text-muted-foreground -mt-1 mb-1">{t("extensions.browserDesc")}</p>
    <SettingsCard>
      <CardContent class="py-0 px-0">
        <div class="divide-y divide-border dark:divide-white/[0.04]">
          {#each extensions as ext}
            <div class="flex items-center justify-between gap-3 px-4 py-3.5">
              <div class="flex items-center gap-3 flex-1 min-w-0">
                <div class="w-8 h-8 flex-shrink-0 flex items-center justify-center rounded-md bg-muted dark:bg-white/[0.06]">
                  <svg viewBox="0 0 24 24" class="w-5 h-5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    {@html ext.icon}
                  </svg>
                </div>
                <div class="min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="font-medium text-sm">{t(ext.nameKey)}</span>
                    {#if ext.installed}
                      <Badge variant="default">{t("extensions.installed")}</Badge>
                    {/if}
                  </div>
                  <p class="text-xs text-muted-foreground mt-0.5 truncate">{t(ext.descKey)}</p>
                  {#if extMessage[ext.id]}
                    <p class="text-xs mt-1 {extMessage[ext.id].type === 'success' ? 'text-green-500' : 'text-red-500'}">
                      {extMessage[ext.id].text}
                    </p>
                  {/if}
                </div>
              </div>
              <div class="flex items-center gap-2 flex-shrink-0">
                {#if ext.id === "safari" && ext.installed}
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={enablingUnsigned}
                    onclick={enableSafariUnsignedExtensions}
                    title="Enable Safari's Develop menu and Allow Unsigned Extensions"
                  >
                    {#if enablingUnsigned}…{:else}{t("extensions.allowUnsigned")}{/if}
                  </Button>
                {/if}
                {#if ext.storeUrl}
                  <Button size="sm" variant="ghost" onclick={() => openStore(ext)}>
                    {t("extensions.openStore")}
                  </Button>
                {/if}
                {#if ext.canAutoInstall}
                  <Button
                    size="sm"
                    variant={ext.installed ? "outline" : "default"}
                    disabled={ext.installing}
                    onclick={() => installExtension(ext)}
                  >
                    {#if ext.installing}
                      {t("extensions.installing")}
                    {:else if ext.installed}
                      {t("extensions.reinstall")}
                    {:else}
                      {t("extensions.install")}
                    {/if}
                  </Button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      </CardContent>
    </SettingsCard>
  </section>

  <!-- Pairing Token -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("extensions.pairingTitle")}</SectionHeader>
    <p class="text-sm text-muted-foreground -mt-1 mb-1">{t("extensions.pairingDesc")}</p>
    <SettingsCard>
      <CardContent class="px-4 py-3.5 flex flex-col gap-3">
        <div class="flex items-center gap-2">
          <Button size="sm" variant="default" onclick={copyPairingToken}>
            {t("extensions.copyPairingToken")}
          </Button>
          <span class="text-xs text-muted-foreground">{t("extensions.copyPairingTokenHint")}</span>
        </div>
        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline" onclick={pairViaBrowser} disabled={pairingInProgress}>
            {#if pairingInProgress}
              {t("extensions.pairingInProgress")}
            {:else}
              {t("extensions.pairViaBrowser")}
            {/if}
          </Button>
          <span class="text-xs text-muted-foreground">{t("extensions.pairViaBrowserHint")}</span>
        </div>
        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline" onclick={copyAuthToken}>
            {t("extensions.copyToken")}
          </Button>
          <span class="text-xs text-muted-foreground">{t("extensions.copyTokenHint")}</span>
        </div>
        {#if statusMessage}
          <span class="text-xs {statusType === 'success' ? 'text-green-500' : 'text-red-500'}">
            {statusMessage}
          </span>
        {/if}
      </CardContent>
    </SettingsCard>
  </section>

</div>

