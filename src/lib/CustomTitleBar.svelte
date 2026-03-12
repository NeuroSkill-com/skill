<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import LanguagePicker from "./LanguagePicker.svelte";
  import ThemeToggle from "./ThemeToggle.svelte";

  let osType: string | null = $state(null);

  $effect(() => {
    const ua = navigator.userAgent;
    if (ua.includes("Mac OS")) {
      osType = "Darwin";
    } else if (ua.includes("Windows")) {
      osType = "Windows";
    } else if (ua.includes("Linux")) {
      osType = "Linux";
    }
  });

  async function minimizeWindow() {
    await getCurrentWindow().minimize();
  }

  async function toggleMaximizeWindow() {
    await getCurrentWindow().toggleMaximize();
  }

  async function closeWindow() {
    await getCurrentWindow().close();
  }

  async function openLabel() {
    await invoke("open_label_window");
  }

  async function openHistory() {
    await invoke("open_history_window");
  }
</script>

<div class="titlebar">
  {#if osType === "Darwin"}
    <!-- macOS: controls on left, spacer, actions on right -->
    <div class="titlebar-controls">
      <button type="button" title="minimize" aria-label="Minimize" onclick={minimizeWindow}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
          <path fill="currentColor" d="M19 13H5v-2h14z" />
        </svg>
      </button>
      <button type="button" title="maximize" aria-label="Maximize" onclick={toggleMaximizeWindow}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
          <path fill="currentColor" d="M4 4h16v16H4zm2 4v10h12V8z" />
        </svg>
      </button>
      <button type="button" title="close" aria-label="Close" onclick={closeWindow}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
          <path
            fill="currentColor"
            d="M13.46 12L19 17.54V19h-1.46L12 13.46L6.46 19H5v-1.46L10.54 12L5 6.46V5h1.46L12 10.54L17.54 5H19v1.46z"
          />
        </svg>
      </button>
    </div>
    <div data-tauri-drag-region class="titlebar-drag-region"></div>
    <div class="titlebar-actions">
      <!-- Label button -->
      <button type="button" title="Add Label" aria-label="Add Label" onclick={openLabel}
        class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
          <path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/>
          <line x1="7" y1="7" x2="7.01" y2="7"/>
        </svg>
      </button>
      <!-- History button -->
      <button type="button" title="History" aria-label="History" onclick={openHistory}
        class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
          <circle cx="12" cy="12" r="10"/>
          <polyline points="12 6 12 12 16 14"/>
        </svg>
      </button>
      <!-- Theme toggle -->
      <ThemeToggle />
      <!-- Language picker -->
      <LanguagePicker />
    </div>
  {:else}
    <!-- Windows/Linux: actions on left, spacer, controls on right -->
    <div class="titlebar-actions">
      <!-- Label button -->
      <button type="button" title="Add Label" aria-label="Add Label" onclick={openLabel}
        class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
          <path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/>
          <line x1="7" y1="7" x2="7.01" y2="7"/>
        </svg>
      </button>
      <!-- History button -->
      <button type="button" title="History" aria-label="History" onclick={openHistory}
        class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
          <circle cx="12" cy="12" r="10"/>
          <polyline points="12 6 12 12 16 14"/>
        </svg>
      </button>
      <!-- Theme toggle -->
      <ThemeToggle />
      <!-- Language picker -->
      <LanguagePicker />
    </div>
    <div data-tauri-drag-region class="titlebar-drag-region"></div>
    <div class="titlebar-controls">
      <button type="button" title="minimize" aria-label="Minimize" onclick={minimizeWindow}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
          <path fill="currentColor" d="M19 13H5v-2h14z" />
        </svg>
      </button>
      <button type="button" title="maximize" aria-label="Maximize" onclick={toggleMaximizeWindow}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
          <path fill="currentColor" d="M4 4h16v16H4zm2 4v10h12V8z" />
        </svg>
      </button>
      <button type="button" title="close" aria-label="Close" onclick={closeWindow}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
          <path
            fill="currentColor"
            d="M13.46 12L19 17.54V19h-1.46L12 13.46L6.46 19H5v-1.46L10.54 12L5 6.46V5h1.46L12 10.54L17.54 5H19v1.46z"
          />
        </svg>
      </button>
    </div>
  {/if}
</div>

<style>
  .titlebar {
    height: 30px;
    background: var(--color-surface);
    user-select: none;
    display: flex;
    align-items: center;
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 1000;
    border-bottom: 1px solid var(--color-border);
    gap: 0;
  }

  .titlebar-drag-region {
    flex: 1;
    cursor: grab;
    pointer-events: auto;
    height: 100%;
  }

  .titlebar-drag-region:active {
    cursor: grabbing;
  }

  .titlebar-actions {
    display: flex;
    gap: 0;
    align-items: center;
    pointer-events: auto;
  }

  .titlebar-controls {
    display: flex;
    gap: 0;
    pointer-events: auto;
  }

  .titlebar button {
    appearance: none;
    padding: 0;
    margin: 0;
    border: none;
    display: inline-flex;
    justify-content: center;
    align-items: center;
    width: 30px;
    height: 30px;
    background-color: transparent;
    color: var(--color-text);
    cursor: pointer;
    transition: background-color 0.2s;
    pointer-events: auto;
  }

  .titlebar-actions button {
    width: 30px;
    height: 30px;
  }

  .titlebar-controls button {
    width: 46px;
    height: 30px;
  }

  .titlebar button:hover {
    background-color: var(--color-hover);
  }

  .titlebar button:active {
    background-color: var(--color-active);
  }

  .titlebar button svg {
    width: 18px;
    height: 18px;
  }
</style>
