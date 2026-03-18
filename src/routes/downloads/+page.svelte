<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { Button } from "$lib/components/ui/button";
  import { t } from "$lib/i18n/index.svelte";
  import { fmtDateTimeLocale, fmtGB } from "$lib/format";

  type DownloadState = "not_downloaded" | "downloading" | "paused" | "downloaded" | "failed" | "cancelled";

  interface DownloadItem {
    repo: string;
    filename: string;
    quant: string;
    size_gb: number;
    description: string;
    is_mmproj: boolean;
    state: DownloadState;
    status_msg: string | null;
    progress: number;
    initiated_at_unix: number | null;
    local_path: string | null;
    shard_count: number;
    current_shard: number;
  }

  let items = $state<DownloadItem[]>([]);
  let loading = $state(true);
  let timer: ReturnType<typeof setInterval> | undefined;

  const totalDownloadsSizeGb = $derived.by(() =>
    items.reduce((sum, item) => sum + (Number.isFinite(item.size_gb) ? item.size_gb : 0), 0)
  );

  async function load() {
    try {
      items = await invoke<DownloadItem[]>("get_llm_downloads");
    } finally {
      loading = false;
    }
  }

  async function cancelItem(filename: string) {
    await invoke("cancel_llm_download", { filename });
    await load();
  }

  async function pauseItem(filename: string) {
    await invoke("pause_llm_download", { filename });
    await load();
  }

  async function resumeItem(filename: string) {
    await invoke("resume_llm_download", { filename });
    await load();
  }

  async function deleteItem(filename: string) {
    await invoke("delete_llm_model", { filename });
    await load();
  }

  const fmtSize = fmtGB;

  function fmtInitiated(unix: number | null): string {
    if (!unix) return t("downloads.initiatedUnknown");
    return fmtDateTimeLocale(unix);
  }

  function statusLabel(s: DownloadState): string {
    if (s === "downloading") return t("downloads.status.downloading");
    if (s === "paused") return t("downloads.status.paused");
    if (s === "downloaded") return t("downloads.status.downloaded");
    if (s === "failed") return t("downloads.status.failed");
    if (s === "cancelled") return t("downloads.status.cancelled");
    return t("downloads.status.notDownloaded");
  }

  onMount(async () => {
    await load();
    timer = setInterval(() => { void load(); }, 1000);
  });

  onDestroy(() => {
    clearInterval(timer);
  });
</script>

<main class="h-full min-h-0 flex flex-col overflow-hidden bg-background">
  <div class="shrink-0 px-3 py-2 border-b border-border dark:border-white/[0.07] bg-card">
    <div class="flex items-center justify-between text-[0.72rem]">
      <span class="font-semibold text-foreground/90">
        Total download size · {items.length} {items.length === 1 ? "item" : "items"}
      </span>
      <span class="tabular-nums font-bold text-foreground">
        {loading ? t("downloads.loading") : fmtSize(totalDownloadsSizeGb)}
      </span>
    </div>
  </div>

  <section class="min-h-0 flex-1 overflow-y-auto p-3">
    {#if loading}
      <p class="text-[0.72rem] text-muted-foreground">{t("downloads.loading")}</p>
    {:else if items.length === 0}
      <p class="text-[0.72rem] text-muted-foreground">{t("downloads.empty")}</p>
    {:else}
      <div class="flex flex-col gap-2">
        {#each items as item (item.filename)}
          <article class="rounded-xl border border-border dark:border-white/[0.08] bg-card px-3 py-2.5">
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0 flex-1">
                <p class="text-[0.72rem] font-semibold text-foreground truncate">{item.filename}</p>
                <p class="text-[0.62rem] text-muted-foreground truncate">{item.description}</p>
                <p class="text-[0.58rem] text-muted-foreground/80 mt-0.5">
                  {item.quant} · {fmtSize(item.size_gb)}{item.shard_count > 1 ? ` (${item.shard_count} parts)` : ""} · {statusLabel(item.state)}{item.shard_count > 1 && item.current_shard > 0 ? ` — part ${item.current_shard}/${item.shard_count}` : ""}
                </p>
                <p class="text-[0.58rem] text-muted-foreground/80">{t("downloads.initiatedAt")}: {fmtInitiated(item.initiated_at_unix)}</p>
                {#if item.status_msg}
                  <p class="text-[0.58rem] text-muted-foreground mt-0.5 truncate">{item.status_msg}</p>
                {/if}
              </div>

              <div class="shrink-0 flex items-center gap-1">
                {#if item.state === "downloading"}
                  <Button size="sm" variant="outline" class="h-6 text-[0.6rem] px-2" onclick={() => pauseItem(item.filename)}>
                    {t("downloads.pause")}
                  </Button>
                  <Button size="sm" variant="outline" class="h-6 text-[0.6rem] px-2 text-red-500 border-red-500/30" onclick={() => cancelItem(item.filename)}>
                    {t("downloads.cancel")}
                  </Button>
                {:else if item.state === "paused"}
                  <Button size="sm" class="h-6 text-[0.6rem] px-2 bg-violet-600 hover:bg-violet-700 text-white" onclick={() => resumeItem(item.filename)}>
                    {t("downloads.resume")}
                  </Button>
                  <Button size="sm" variant="outline" class="h-6 text-[0.6rem] px-2 text-red-500 border-red-500/30" onclick={() => cancelItem(item.filename)}>
                    {t("downloads.cancel")}
                  </Button>
                {:else}
                  <Button size="sm" variant="ghost" class="h-6 text-[0.6rem] px-2 text-red-500" onclick={() => deleteItem(item.filename)}>
                    {t("downloads.delete")}
                  </Button>
                {/if}
              </div>
            </div>

            {#if item.state === "downloading" || item.state === "paused"}
              <div class="mt-2 h-1 w-full rounded-full bg-muted overflow-hidden">
                <div class="h-full rounded-full bg-blue-500 transition-all duration-200" style="width:{(Math.max(0, Math.min(1, item.progress)) * 100).toFixed(1)}%"></div>
              </div>
            {/if}
          </article>
        {/each}
      </div>
    {/if}
  </section>

</main>
