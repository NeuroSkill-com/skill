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

let extensions = $state<ExtensionInfo[]>([
  {
    id: "vscode",
    nameKey: "extensions.vscode",
    descKey: "extensions.vscodeDesc",
    icon: `<path d="M17.583 2.237L10.82 8.363 5.94 4.657 3.5 5.726v12.548l2.44 1.069 4.88-3.706 6.763 6.126L20.5 19.92V4.08l-2.917-1.843zM10 12l-4.146 3.291V8.709L10 12zm7.5 4.846L13.038 12l4.462-4.846v9.692z"/>`,
    installed: false,
    installing: false,
    storeUrl: "https://marketplace.visualstudio.com/items?itemName=neuroskill.neuroskill",
    canAutoInstall: true,
  },
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

let statusMessage = $state("");
let statusType = $state<"success" | "error" | "">("");

// ── Actions ──────────────────────────────────────────────────────────────────

async function installExtension(ext: ExtensionInfo): Promise<void> {
  const idx = extensions.findIndex((e) => e.id === ext.id);
  if (idx < 0) return;
  extensions[idx].installing = true;
  statusMessage = "";

  try {
    const result = await invoke<{ ok: boolean; message: string }>("install_extension", {
      extensionId: ext.id,
    });
    if (result.ok) {
      extensions[idx].installed = true;
      statusMessage = result.message;
      statusType = "success";
    } else {
      statusMessage = result.message;
      statusType = "error";
    }
  } catch (e: any) {
    statusMessage = e.message ?? String(e);
    statusType = "error";
  }
  extensions[idx].installing = false;
}

async function openStore(ext: ExtensionInfo): Promise<void> {
  if (ext.storeUrl) {
    await openUrl(ext.storeUrl);
  }
}

async function checkInstalled(): Promise<void> {
  try {
    const result = await invoke<Record<string, boolean>>("check_extensions_installed");
    for (let i = 0; i < extensions.length; i++) {
      if (result[extensions[i].id] !== undefined) {
        extensions[i].installed = result[extensions[i].id];
      }
    }
  } catch {
    // Backend may not implement this yet — fail silently
  }
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

// Check status on mount
$effect(() => {
  checkInstalled();
});
</script>

<div class="flex flex-col gap-5">

  <!-- IDE Extensions -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("extensions.ideTitle")}</SectionHeader>
    <SettingsCard>
      <CardContent class="py-0 px-0">
        <div class="divide-y divide-border dark:divide-white/[0.04]">
          {#each extensions.filter((e) => e.id === "vscode") as ext}
            <div class="flex items-center justify-between gap-3 px-4 py-3.5">
              <div class="flex items-center gap-3 flex-1 min-w-0">
                <div class="w-8 h-8 flex-shrink-0 flex items-center justify-center rounded-md bg-muted dark:bg-white/[0.06]">
                  <svg viewBox="0 0 24 24" class="w-5 h-5" fill="currentColor" stroke="none">
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
                </div>
              </div>
              <div class="flex items-center gap-2 flex-shrink-0">
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

  <!-- Browser Extensions -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("extensions.browserTitle")}</SectionHeader>
    <p class="text-sm text-muted-foreground -mt-1 mb-1">{t("extensions.browserDesc")}</p>
    <SettingsCard>
      <CardContent class="py-0 px-0">
        <div class="divide-y divide-border dark:divide-white/[0.04]">
          {#each extensions.filter((e) => e.id !== "vscode") as ext}
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
                </div>
              </div>
              <div class="flex items-center gap-2 flex-shrink-0">
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
      <CardContent class="px-4 py-3.5">
        <Button size="sm" variant="outline" onclick={copyAuthToken}>
          {t("extensions.copyToken")}
        </Button>
        {#if statusMessage}
          <span class="ml-3 text-xs {statusType === 'success' ? 'text-green-500' : 'text-red-500'}">
            {statusMessage}
          </span>
        {/if}
      </CardContent>
    </SettingsCard>
  </section>

</div>

