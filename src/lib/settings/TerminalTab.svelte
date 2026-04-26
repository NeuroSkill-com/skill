<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Terminal integration — install/uninstall shell hooks for command tracking. -->
<script lang="ts">
import { onMount } from "svelte";
import { Button } from "$lib/components/ui/button";
import { CardContent } from "$lib/components/ui/card";
import SectionHeader from "$lib/components/ui/section-header/SectionHeader.svelte";
import SettingsCard from "$lib/components/ui/settings-card/SettingsCard.svelte";
import { Spinner } from "$lib/components/ui/spinner";
import { daemonGet, daemonPost } from "$lib/daemon/http";

interface ShellStatus {
  shell: string;
  label: string;
  rcFile: string;
  hookFile: string;
  installed: boolean;
  hookExists: boolean;
  rcHasLine: boolean;
  available: boolean;
}

interface TerminalCommand {
  id: number;
  terminal_name: string;
  command: string;
  cwd: string;
  exit_code: number | null;
  started_at: number;
  category: string;
}

let loading = $state(true);
let shells = $state<ShellStatus[]>([]);
let recentCommands = $state<TerminalCommand[]>([]);
let installing = $state<string | null>(null);
let error = $state("");
let success = $state("");

const SHELLS = [
  { shell: "zsh", label: "Zsh", rc: ".zshrc" },
  { shell: "bash", label: "Bash", rc: ".bashrc" },
  { shell: "fish", label: "Fish", rc: "config.fish" },
  { shell: "powershell", label: "PowerShell", rc: "$PROFILE" },
];

onMount(async () => {
  await refresh();
});

async function refresh() {
  loading = true;
  error = "";
  try {
    // Check status of each shell hook
    const statuses: ShellStatus[] = [];
    for (const s of SHELLS) {
      try {
        const status = await daemonPost<any>("/v1/activity/shell-hook-status", { shell: s.shell });
        statuses.push({
          shell: s.shell,
          label: s.label,
          rcFile: status?.rc_file ?? s.rc,
          hookFile: status?.hook_path ?? "",
          installed: status?.installed ?? false,
          hookExists: status?.hook_exists ?? false,
          rcHasLine: status?.rc_has_line ?? false,
          available: status?.available ?? true,
        });
      } catch {
        statuses.push({
          shell: s.shell,
          label: s.label,
          rcFile: s.rc,
          hookFile: "",
          installed: false,
          hookExists: false,
          rcHasLine: false,
          available: false,
        });
      }
    }
    shells = statuses;

    // Fetch recent terminal commands
    try {
      const now = Math.floor(Date.now() / 1000);
      recentCommands = await daemonPost<TerminalCommand[]>("/v1/brain/terminal-commands", {
        since: now - 3600,
        limit: 15,
      });
    } catch {
      recentCommands = [];
    }
  } catch (e: any) {
    error = e?.message ?? "Failed to load terminal status";
  }
  loading = false;
}

async function install(shell: string) {
  installing = shell;
  error = "";
  success = "";
  try {
    const result = await daemonPost<any>("/v1/activity/install-shell-hook", { shell });
    if (result?.ok) {
      if (result.instructions) {
        // PowerShell returns manual setup instructions instead of touching $PROFILE.
        success = result.instructions;
      } else if (result.already_installed) {
        success = result.hook_refreshed
          ? `${shell} hook refreshed in ${result.rc_file}. Open a new terminal to pick up the latest version.`
          : `${shell} hook already installed in ${result.rc_file}`;
      } else {
        success = `${shell} hook installed in ${result.rc_file ?? result.hook_path}. Open a new terminal for it to take effect.`;
      }
      await refresh();
    } else {
      error = result?.error ?? "Installation failed";
    }
  } catch (e: any) {
    error = e?.message ?? "Installation failed";
    console.error("install hook failed:", e);
  }
  installing = null;
}

async function uninstall(shell: string) {
  installing = shell;
  error = "";
  success = "";
  try {
    const result = await daemonPost<any>("/v1/activity/uninstall-shell-hook", { shell });
    if (result?.ok) {
      success = `${shell} hook removed. Open a new terminal to apply.`;
      await refresh();
    } else {
      error = result?.error ?? "Uninstall failed";
    }
  } catch (e: any) {
    error = e?.message ?? "Uninstall failed";
  }
  installing = null;
}

function fmtTime(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function categoryColor(cat: string): string {
  const colors: Record<string, string> = {
    build: "text-blue-400", test: "text-purple-400", run: "text-green-400",
    git: "text-orange-400", docker: "text-cyan-400", deploy: "text-yellow-400",
    install: "text-pink-400", navigate: "text-muted-foreground", debug: "text-red-400",
  };
  return colors[cat] ?? "text-muted-foreground";
}
</script>

{#if loading}
  <div class="flex items-center justify-center py-12"><Spinner /></div>
{:else}
<div class="flex flex-col gap-5">

  <!-- Status messages -->
  {#if error}
    <div class="rounded-md border border-red-500/30 bg-red-500/5 px-4 py-2 text-sm text-red-400">{error}</div>
  {/if}
  {#if success}
    <div class="rounded-md border border-green-500/30 bg-green-500/5 px-4 py-2 text-xs text-green-400 whitespace-pre-line break-all font-mono leading-relaxed">{success}</div>
  {/if}

  <!-- Shell hooks -->
  <SettingsCard>
    <SectionHeader
      title="Shell Hooks"
      description="Install tracking hooks into your shell to capture every terminal command. Works in any terminal app."
    />
    <CardContent class="space-y-3">
      {#each shells as s}
        <div class="flex items-center justify-between gap-3 rounded-lg border border-border/50 px-4 py-3">
          <div class="flex min-w-0 flex-1 items-center gap-3">
            <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted text-xs font-bold">
              {s.label.slice(0, 2)}
            </div>
            <div class="min-w-0 flex-1">
              <div class="text-sm font-medium">{s.label}</div>
              <div class="truncate text-xs text-muted-foreground" title={s.rcFile}>
                {#if s.installed}
                  <span class="text-green-400">Installed</span> &middot; {s.rcFile}
                {:else if !s.available}
                  <span class="text-muted-foreground">Not available on this system</span>
                {:else}
                  <span class="text-yellow-400">Not installed</span>
                {/if}
              </div>
            </div>
          </div>
          <div class="flex shrink-0 items-center gap-2">
            {#if s.installed}
              <!-- Health indicator -->
              {#if s.hookExists && s.rcHasLine}
                <span class="h-2 w-2 rounded-full bg-green-400" title="Hook file and rc line both present"></span>
              {:else if !s.hookExists}
                <span class="h-2 w-2 rounded-full bg-red-400" title="Hook file missing — reinstall"></span>
              {:else if !s.rcHasLine}
                <span class="h-2 w-2 rounded-full bg-yellow-400" title="Source line missing from rc file"></span>
              {/if}
              <Button variant="outline" size="sm"
                onclick={() => install(s.shell)}
                disabled={installing === s.shell}>
                {installing === s.shell ? "..." : "Repair"}
              </Button>
              <Button variant="destructive" size="sm"
                onclick={() => uninstall(s.shell)}
                disabled={installing === s.shell}>
                {installing === s.shell ? "..." : "Remove"}
              </Button>
            {:else if s.available}
              <Button variant="default" size="sm"
                onclick={() => install(s.shell)}
                disabled={installing === s.shell}>
                {installing === s.shell ? "Installing..." : "Install"}
              </Button>
            {/if}
          </div>
        </div>
      {/each}
    </CardContent>
  </SettingsCard>

  <!-- How it works -->
  <SettingsCard>
    <SectionHeader
      title="How it works"
      description="Shell hooks run a background curl on every command — no delay to your prompt."
    />
    <CardContent>
      <div class="space-y-2 text-sm text-muted-foreground">
        <p>When you press Enter in your terminal, the hook sends the command text, working directory, and exit code to the NeuroSkill daemon. This enables:</p>
        <ul class="ml-4 list-disc space-y-1">
          <li>Build/test failure detection correlated with your brain state</li>
          <li>Dev loop analysis (edit-build-test cycles)</li>
          <li>Terminal command EEG auto-labels for searchable recordings</li>
          <li>Struggle prediction based on repeated failing commands</li>
          <li>Task type detection (testing, deploying, debugging, etc.)</li>
        </ul>
        <p class="mt-2">Works in any terminal: VS Code, iTerm, Terminal.app, Warp, tmux, ssh sessions.</p>
      </div>
    </CardContent>
  </SettingsCard>

  <!-- Recent commands -->
  <SettingsCard>
    <SectionHeader
      title="Recent Commands"
      description="Last hour of tracked terminal activity"
    />
    <CardContent>
      {#if recentCommands.length > 0}
        <div class="space-y-1">
          {#each recentCommands as cmd}
            <div class="flex min-w-0 items-center gap-2 rounded px-2 py-1 text-xs font-mono hover:bg-muted/50">
              <span class="w-16 shrink-0 text-muted-foreground">{fmtTime(cmd.started_at)}</span>
              <span class="w-5 shrink-0 text-center">
                {#if cmd.exit_code === null}
                  <span class="text-yellow-400">~</span>
                {:else if cmd.exit_code === 0}
                  <span class="text-green-400">ok</span>
                {:else}
                  <span class="text-red-400">!</span>
                {/if}
              </span>
              <span class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] {categoryColor(cmd.category)}">{cmd.category}</span>
              <span class="min-w-0 flex-1 truncate" title={cmd.command}>{cmd.command}</span>
            </div>
          {/each}
        </div>
      {:else}
        <p class="text-sm text-muted-foreground italic">No commands tracked yet. Install a shell hook and open a new terminal.</p>
      {/if}
    </CardContent>
  </SettingsCard>

  <div class="flex justify-end">
    <Button variant="outline" size="sm" onclick={refresh}>Refresh</Button>
  </div>
</div>
{/if}
