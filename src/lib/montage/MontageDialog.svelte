<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Montages: with `csvPath`, the montage of that recording (see, replace, remove);
     without, the library of the settings and which montage is whose default. -->

<script lang="ts">
import { Button } from "#lib/components/ui/button";
import * as Dialog from "#lib/components/ui/dialog";
import { Textarea } from "#lib/components/ui/textarea";
import {
  deleteMontage,
  getMontages,
  getSessionMontage,
  type Montage,
  type MontageRule,
  previewMontage,
  type ResolvedMontage,
  saveMontage,
  setSessionMontage,
} from "#lib/daemon/montage";
import { t } from "#lib/i18n/index.svelte";

interface Props {
  open: boolean;
  /** A recording: the dialog is about its montage. Absent: about the library. */
  csvPath?: string | null;
}

let { open = $bindable(), csvPath = null }: Props = $props();

const inputClass = "w-full min-w-0 text-ui-md px-2 py-1 rounded-md border border-border bg-background text-foreground";

let library = $state<Montage[]>([]);
let defaults = $state<MontageRule[]>([]);
/** What is in force now: the recording's channels, or nothing for the library. */
let current = $state<ResolvedMontage | null>(null);
let origin = $state<"default" | "session" | null>(null);
/** The text being edited, laid over the channels. */
let text = $state("");
let preview = $state<ResolvedMontage | null>(null);
let error = $state("");
let busy = $state(false);

let name = $state("");
let asDefault = $state(false);
let deviceName = $state("");
let channelCount = $state("");

async function load() {
  error = "";
  preview = null;
  const lib = await getMontages().catch(() => null);
  if (lib?.ok) {
    library = lib.library;
    defaults = lib.defaults;
  }
  if (csvPath) {
    const s = await getSessionMontage(csvPath).catch(() => null);
    if (s?.ok) {
      current = s.resolved;
      origin = s.origin;
    } else {
      current = null;
      error = s && !s.ok ? s.error : "";
    }
  }
}

$effect(() => {
  if (open) {
    text = "";
    name = "";
    asDefault = false;
    void load();
  }
});

// The preview follows the text, a moment after the last keystroke.
let timer: ReturnType<typeof setTimeout> | undefined;
$effect(() => {
  const spec = text.trim();
  const n = Number.parseInt(channelCount, 10);
  clearTimeout(timer);
  if (!spec) {
    preview = null;
    error = "";
    return;
  }
  timer = setTimeout(async () => {
    const over = csvPath
      ? { csvPath }
      : n > 0
        ? { channelNames: Array.from({ length: n }, (_, k) => `CH${k + 1}`) }
        : {};
    const r = await previewMontage(spec, over).catch(() => null);
    if (r?.ok) {
      preview = r.resolved;
      error = "";
    } else {
      preview = null;
      error = r ? r.error : "";
    }
  }, 250);
});

const shown = $derived(preview ?? current);

function ruleText(r: MontageRule): string {
  const parts = [r.device_name, r.device_kind, r.channel_count ? `${r.channel_count} ch` : ""].filter(Boolean);
  return parts.length ? parts.join(" · ") : t("settings.montageAnyDevice");
}

async function apply(spec: string | null) {
  if (!csvPath) return;
  busy = true;
  const r = await setSessionMontage(csvPath, spec).catch(() => null);
  busy = false;
  if (r?.ok) {
    current = r.resolved;
    origin = r.origin;
    text = "";
  } else {
    error = r ? r.error : "";
  }
}

async function save() {
  busy = true;
  const n = Number.parseInt(channelCount, 10);
  const r = await saveMontage(
    name,
    text,
    asDefault ? { deviceName: deviceName.trim() || undefined, channelCount: n > 0 ? n : undefined } : undefined,
  ).catch(() => null);
  busy = false;
  if (r?.ok) {
    library = r.library;
    defaults = r.defaults;
    if (!csvPath) {
      text = "";
      name = "";
    }
  } else {
    error = r ? r.error : "";
  }
}

async function remove(montage: string) {
  const r = await deleteMontage(montage).catch(() => null);
  if (r?.ok) {
    library = r.library;
    defaults = r.defaults;
  }
}
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-xl max-h-[85vh] overflow-y-auto">
    <Dialog.Header>
      <Dialog.Title>{t("settings.montage")}</Dialog.Title>
      <Dialog.Description>
        {#if csvPath}
          {origin === "default"
            ? t("settings.montageOriginDefault")
            : origin === "session"
              ? t("settings.montageOriginSession")
              : t("settings.montageOriginNone")}
        {:else}
          {t("settings.montageDesc")}
        {/if}
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-3">
      <!-- Saved montages: pick one for the recording, or manage the library -->
      {#if library.length > 0}
        <div class="flex flex-col gap-1">
          <span class="text-ui-sm text-muted-foreground">{t("settings.montageLibrary")}</span>
          {#each library as m (m.name)}
            <div class="flex items-center gap-2 rounded-md border border-border px-2 py-1">
              <span class="text-ui-md font-medium truncate">{m.name}</span>
              <span class="text-ui-xs text-muted-foreground shrink-0">{m.channels.length} ch</span>
              <span class="text-ui-xs text-muted-foreground truncate flex-1">
                {defaults.filter((r) => r.montage.toLowerCase() === m.name.toLowerCase()).map(ruleText).join("; ")}
              </span>
              {#if csvPath}
                <Button size="sm" variant="outline" class="h-6 text-ui-sm" disabled={busy} onclick={() => apply(m.name)}>
                  {t("settings.montageApply")}
                </Button>
              {:else}
                <Button size="sm" variant="ghost" class="h-6 text-ui-sm" onclick={() => { text = JSON.stringify(m, null, 1); name = m.name; }}>
                  {t("settings.montageEdit")}
                </Button>
                <Button size="sm" variant="ghost" class="h-6 text-ui-sm text-destructive" onclick={() => remove(m.name)}>
                  {t("settings.montageDelete")}
                </Button>
              {/if}
            </div>
          {/each}
        </div>
      {:else if !csvPath}
        <p class="text-ui-sm text-muted-foreground">{t("settings.montageNone")}</p>
      {/if}

      <!-- Write one -->
      <label class="flex flex-col gap-1">
        <span class="text-ui-sm text-muted-foreground">{t("settings.montageChannels")}</span>
        <Textarea bind:value={text} rows={3} spellcheck={false} class="font-mono text-ui-sm"
                  placeholder="Fp1 Fp2 F7 F3 Fz F4 F8 …   CH32=HEOG:eog   CH7=T7!" />
        <span class="text-ui-xs text-muted-foreground leading-snug">{t("settings.montageChannelsHint")}</span>
      </label>

      {#if error}
        <p class="text-ui-sm text-destructive">{error}</p>
      {/if}

      <!-- The channels as they would be (or are) read -->
      {#if shown}
        <div class="flex flex-col gap-1">
          <span class="text-ui-sm text-muted-foreground">
            {preview ? t("settings.montagePreview") : t("settings.montageChannels")} ·
            {t("settings.montageCovered", { n: shown.covered, total: shown.channels.length })}
            {#if shown.reference}· ref {shown.reference}{/if}
          </span>
          <div class="flex flex-wrap gap-1 max-h-40 overflow-y-auto">
            {#each shown.channels as c (c.index)}
              <span class="rounded border px-1.5 py-0.5 text-ui-xs font-mono
                           {c.bad ? 'border-destructive/50 text-destructive line-through' : c.from_montage ? 'border-violet-500/40' : 'border-border text-muted-foreground'}"
                    title="{c.index + 1}: {c.recorded} · {c.kind}">
                {c.label}{c.kind !== "eeg" ? ` (${c.kind})` : ""}
              </span>
            {/each}
          </div>
          {#each shown.warnings as w}
            <p class="text-ui-xs text-amber-600 dark:text-amber-400">{w}</p>
          {/each}
        </div>
      {/if}

      <!-- Keep it: in the library, and as a default -->
      <div class="flex flex-col gap-2 rounded-md border border-border p-2">
        <div class="flex items-end gap-2">
          <label class="flex flex-col gap-1 flex-1">
            <span class="text-ui-sm text-muted-foreground">{t("settings.montageName")}</span>
            <input type="text" bind:value={name} class={inputClass} />
          </label>
          <Button size="sm" variant="outline" class="h-8" disabled={busy || !name.trim() || !text.trim()} onclick={save}>
            {t("settings.montageSave")}
          </Button>
        </div>
        <label class="flex items-center gap-2 text-ui-sm">
          <input type="checkbox" bind:checked={asDefault} />
          {t("settings.montageDefaultFor")}
        </label>
        {#if asDefault}
          <div class="grid grid-cols-2 gap-2">
            <label class="flex flex-col gap-1">
              <span class="text-ui-xs text-muted-foreground">{t("settings.montageDeviceName")}</span>
              <input type="text" bind:value={deviceName} placeholder="EN32*" class={inputClass} />
            </label>
            <label class="flex flex-col gap-1">
              <span class="text-ui-xs text-muted-foreground">{t("settings.montageChannelCount")}</span>
              <input type="number" min="1" bind:value={channelCount} placeholder="32" class={inputClass} />
            </label>
          </div>
        {/if}
      </div>
    </div>

    <Dialog.Footer>
      {#if csvPath}
        <Button variant="ghost" disabled={busy || origin === null} onclick={() => apply(null)}>
          {t("settings.montageRemove")}
        </Button>
        <Button disabled={busy || !text.trim() || !!error} onclick={() => apply(text)}>
          {t("settings.montageApply")}
        </Button>
      {/if}
      <Button variant="outline" onclick={() => { open = false; }}>{t("common.close")}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
