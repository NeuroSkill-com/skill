<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Shortcuts tab — Global Shortcuts · In-App Shortcuts · Command Palette -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onDestroy, onMount } from "svelte";
import { Badge } from "$lib/components/ui/badge";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import { t } from "$lib/i18n/index.svelte";

// ── State ──────────────────────────────────────────────────────────────────
let shortcut = $state("");
let searchShortcut = $state("");
let settingsShortcut = $state("");
let calibrationShortcut = $state("");
let helpShortcut = $state("");
let historyShortcut = $state("");
let apiShortcut = $state("");
let themeShortcut = $state("");
let focusTimerShortcut = $state("");
let chatShortcut = $state("");
let compareShortcut = $state("");
let shortcutError = $state("");
let recording = $state(false);
let recordingTarget = $state<string>("label");

// ── Helpers ────────────────────────────────────────────────────────────────
const isMac = typeof navigator !== "undefined" && navigator.platform?.includes("Mac");

function shortcutTokens(acc: string): string[] {
  return acc.split("+").map((p) => {
    if (p === "CmdOrCtrl") return isMac ? "⌘" : "Ctrl";
    if (p === "Meta") return isMac ? "⌘" : "Win";
    if (p === "Shift") return isMac ? "⇧" : "Shift";
    if (p === "Alt") return isMac ? "⌥" : "Alt";
    if (p === "Ctrl") return "Ctrl";
    return p;
  });
}

function keyEventToAccelerator(e: KeyboardEvent): string | null {
  const ONLY_MODS = ["Control", "Meta", "Alt", "Shift", "AltGraph", "CapsLock", "NumLock", "OS"];
  if (ONLY_MODS.includes(e.key)) return null;
  if (!e.ctrlKey && !e.metaKey && !e.altKey) return null;

  const parts: string[] = [];
  if (e.metaKey && isMac) parts.push("CmdOrCtrl");
  else if (e.ctrlKey && !isMac) parts.push("CmdOrCtrl");
  else if (e.ctrlKey) parts.push("Ctrl");
  else if (e.metaKey) parts.push("Meta");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");

  const code = e.code;
  let key: string | null = null;
  if (code.startsWith("Key")) key = code.slice(3);
  else if (code.startsWith("Digit")) key = code.slice(5);
  else if (/^F\d+$/.test(code)) key = code;
  else {
    const MAP: Record<string, string> = {
      Space: "Space",
      Enter: "Return",
      Tab: "Tab",
      Escape: "Escape",
      Backspace: "Backspace",
      Delete: "Delete",
      Insert: "Insert",
      Home: "Home",
      End: "End",
      PageUp: "PageUp",
      PageDown: "PageDown",
      ArrowUp: "Up",
      ArrowDown: "Down",
      ArrowLeft: "Left",
      ArrowRight: "Right",
      Minus: "Minus",
      Equal: "Equal",
      BracketLeft: "LeftBracket",
      BracketRight: "RightBracket",
      Semicolon: "Semicolon",
      Quote: "Quote",
      Comma: "Comma",
      Period: "Period",
      Slash: "Slash",
    };
    key = MAP[code] ?? null;
  }
  if (!key) return null;
  parts.push(key);
  return parts.join("+");
}

const SHORTCUT_COMMANDS: Record<string, string> = {
  label: "set_label_shortcut",
  search: "set_search_shortcut",
  settings: "set_settings_shortcut",
  calibration: "set_calibration_shortcut",
  help: "set_help_shortcut",
  history: "set_history_shortcut",
  api: "set_api_shortcut",
  theme: "set_theme_shortcut",
  focusTimer: "set_focus_timer_shortcut",
  chat: "set_chat_shortcut",
  compare: "set_compare_shortcut",
};

function getShortcutValue(target: string): string {
  if (target === "label") return shortcut;
  if (target === "search") return searchShortcut;
  if (target === "settings") return settingsShortcut;
  if (target === "calibration") return calibrationShortcut;
  if (target === "help") return helpShortcut;
  if (target === "history") return historyShortcut;
  if (target === "api") return apiShortcut;
  if (target === "theme") return themeShortcut;
  if (target === "focusTimer") return focusTimerShortcut;
  if (target === "chat") return chatShortcut;
  if (target === "compare") return compareShortcut;
  return "";
}

function setShortcutValue(target: string, val: string) {
  if (target === "label") shortcut = val;
  if (target === "search") searchShortcut = val;
  if (target === "settings") settingsShortcut = val;
  if (target === "calibration") calibrationShortcut = val;
  if (target === "help") helpShortcut = val;
  if (target === "history") historyShortcut = val;
  if (target === "api") apiShortcut = val;
  if (target === "theme") themeShortcut = val;
  if (target === "focusTimer") focusTimerShortcut = val;
  if (target === "chat") chatShortcut = val;
  if (target === "compare") compareShortcut = val;
}

async function applyShortcut(acc: string, target: string = recordingTarget) {
  shortcutError = "";
  try {
    await invoke(SHORTCUT_COMMANDS[target], { shortcut: acc });
    setShortcutValue(target, acc);
    recording = false;
  } catch (e) {
    shortcutError = String(e);
  }
}

async function clearShortcutFor(target: string) {
  await applyShortcut("", target);
}

function startRecording(target: string) {
  recordingTarget = target;
  recording = true;
  shortcutError = "";
}

function onRecordKeydown(e: KeyboardEvent) {
  if (!recording) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape") {
    recording = false;
    return;
  }
  const acc = keyEventToAccelerator(e);
  if (acc) applyShortcut(acc, recordingTarget);
}

// ── Lifecycle ──────────────────────────────────────────────────────────────
onMount(async () => {
  shortcut = await invoke<string>("get_label_shortcut");
  searchShortcut = await invoke<string>("get_search_shortcut");
  settingsShortcut = await invoke<string>("get_settings_shortcut");
  calibrationShortcut = await invoke<string>("get_calibration_shortcut");
  helpShortcut = await invoke<string>("get_help_shortcut");
  historyShortcut = await invoke<string>("get_history_shortcut");
  apiShortcut = await invoke<string>("get_api_shortcut");
  themeShortcut = await invoke<string>("get_theme_shortcut");
  focusTimerShortcut = await invoke<string>("get_focus_timer_shortcut");
  chatShortcut = await invoke<string>("get_chat_shortcut");
  compareShortcut = await invoke<string>("get_compare_shortcut");
});
</script>

<!-- ── Global Shortcuts ──────────────────────────────────────────────────────── -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<section class="flex flex-col gap-2" onkeydown={onRecordKeydown}>
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("settings.globalShortcuts")}
    </span>
    <span class="ml-auto text-[0.56rem] text-muted-foreground/60">{t("settings.hotkeys")}</span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      {#each ([
        ["label",       t("settings.shortcutAddLabel"),    shortcut],
        ["search",      t("settings.shortcutSearch"),      searchShortcut],
        ["calibration", t("settings.shortcutCalibration"), calibrationShortcut],
        ["settings",    t("settings.shortcutSettings"),    settingsShortcut],
        ["help",        t("settings.shortcutHelp"),        helpShortcut],
        ["history",     t("settings.shortcutHistory"),     historyShortcut],
        ["api",         t("settings.shortcutApi"),         apiShortcut],
        ["theme",       t("settings.shortcutTheme"),       themeShortcut],
        ["focusTimer",  t("settings.shortcutFocusTimer"),  focusTimerShortcut],
        ["chat",        t("settings.shortcutChat"),        chatShortcut],
        ["compare",     t("settings.shortcutCompare"),     compareShortcut],
      ] as [string, string, string][]) as [target, label, value]}
        <div class="flex items-center gap-3 px-4 py-2.5">
          <span class="text-[0.72rem] font-semibold text-foreground w-[120px] shrink-0">{label}</span>

          <div class="flex-1 flex items-center gap-1.5 min-w-0">
            {#if recording && recordingTarget === target}
              <span class="animate-pulse text-[0.68rem] text-muted-foreground italic">
                {t("settings.pressKeysEsc")}
              </span>
            {:else if value}
              {#each shortcutTokens(value) as token}
                <span class="inline-flex items-center justify-center px-1.5 py-0.5 rounded
                             text-[0.64rem] font-semibold leading-none
                             bg-muted dark:bg-white/[0.08]
                             border border-border dark:border-white/[0.12]
                             text-foreground shadow-[inset_0_-1px_0_0] shadow-border">
                  {token}
                </span>
              {/each}
            {:else}
              <span class="text-[0.68rem] text-muted-foreground/50 italic">{t("settings.notSet")}</span>
            {/if}
          </div>

          {#if shortcutError && recordingTarget === target}
            <span class="text-[0.62rem] text-destructive truncate max-w-[100px]" title={shortcutError}>
              {shortcutError}
            </span>
          {/if}

          <div class="flex gap-1 shrink-0">
            {#if value && !(recording && recordingTarget === target)}
              <Button variant="outline" size="sm"
                      class="text-[0.65rem] h-6 px-2 text-muted-foreground"
                      onclick={() => clearShortcutFor(target)}>
                {t("common.clear")}
              </Button>
            {/if}
            {#if recording && recordingTarget === target}
              <Button variant="destructive" size="sm"
                      class="text-[0.65rem] h-6 px-2"
                      onclick={() => { recording = false; shortcutError = ""; }}>
                {t("common.cancel")}
              </Button>
            {:else}
              <Button variant="outline" size="sm"
                      class="text-[0.65rem] h-6 px-2"
                      onclick={() => startRecording(target as any)}>
                {value ? t("common.change") : t("common.record")}
              </Button>
            {/if}
          </div>
        </div>
      {/each}

    </CardContent>
  </Card>
</section>

<!-- ── Command Palette ────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("shortcutsTab.commandPalette")}
    </span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">
      <div class="flex items-center gap-3 px-4 py-3">
        <div class="flex flex-col gap-1 flex-1">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("shortcutsTab.cmdKTitle")}</span>
          <span class="text-[0.68rem] text-muted-foreground leading-relaxed">
            {t("shortcutsTab.cmdKDesc")}
          </span>
        </div>
        <kbd class="inline-flex items-center gap-0.5 rounded-md border border-border
                    dark:border-white/[0.1] bg-muted dark:bg-white/[0.05]
                    px-2 py-0.5 font-mono text-[0.65rem] font-medium
                    text-muted-foreground whitespace-nowrap shrink-0">
          {isMac ? "⌘" : "Ctrl"}K
        </kbd>
      </div>
    </CardContent>
  </Card>
</section>

<!-- ── In-App Shortcuts Reference ─────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("shortcutsTab.inAppShortcuts")}
    </span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      {#each ([
        ["?",                          t("shortcuts.showOverlay")],
        [isMac ? "⌘K" : "Ctrl+K",     t("shortcutsTab.cmdKTitle")],
        ["Esc",                        t("shortcuts.closeOverlay")],
        [isMac ? "⌘↵" : "Ctrl+↵",    t("shortcuts.submitLabel")],
      ] as [string, string][]) as [keys, label]}
        <div class="flex items-center gap-3 px-4 py-2.5">
          <span class="text-[0.72rem] text-foreground/80 flex-1">{label}</span>
          <kbd class="inline-flex items-center gap-0.5 rounded-md border border-border
                      dark:border-white/[0.1] bg-muted dark:bg-white/[0.05]
                      px-2 py-0.5 font-mono text-[0.65rem] font-medium
                      text-muted-foreground whitespace-nowrap shrink-0">
            {keys}
          </kbd>
        </div>
      {/each}

    </CardContent>
  </Card>
</section>
