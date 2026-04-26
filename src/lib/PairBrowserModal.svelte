<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->

<!--
  Approval modal for browser-extension deep-link pairing.
  Shows when a `neuroskill://pair-browser?session_id=...&name=...` deep link arrives.
  User clicks Approve → daemon stores (session_id → auth_token) → extension polls and claims.
-->
<script lang="ts">
import { onMount } from "svelte";
import { daemonPost } from "$lib/daemon/http";
import { addToast } from "$lib/stores/toast.svelte";

let visible = $state(false);
let sessionId = $state("");
let clientName = $state("Browser Extension");
let approving = $state(false);

function checkLocation(): void {
  const hash = window.location.hash;
  if (!hash.includes("pair-browser")) return;

  // The deep-link handler appends ?session_id=...&name=... to the route
  const queryStart = hash.indexOf("?");
  if (queryStart < 0) return;
  const params = new URLSearchParams(hash.slice(queryStart + 1));
  const sid = params.get("session_id");
  const name = params.get("name");
  if (!sid || sid.length < 16) return;

  sessionId = sid;
  clientName = name || "Browser Extension";
  visible = true;
}

onMount(() => {
  checkLocation();
  // Re-check on hash change (deep-link arriving while the app is already open)
  window.addEventListener("hashchange", checkLocation);
  return () => window.removeEventListener("hashchange", checkLocation);
});

async function approve(): Promise<void> {
  approving = true;
  try {
    await daemonPost("/v1/pair/approve", { session_id: sessionId, name: clientName });
    addToast("success", "Browser paired", `${clientName} can now connect to NeuroSkill.`);
    cleanup();
  } catch (e) {
    addToast("error", "Pairing failed", e instanceof Error ? e.message : String(e));
    approving = false;
  }
}

function deny(): void {
  cleanup();
}

function cleanup(): void {
  visible = false;
  approving = false;
  // Clear the hash so re-opening the app doesn't reshow the prompt
  if (window.location.hash.includes("pair-browser")) {
    history.replaceState(null, "", window.location.pathname);
  }
}
</script>

{#if visible}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
    <div class="bg-background border border-border rounded-xl p-6 max-w-md mx-4 shadow-2xl">
      <div class="flex items-center gap-3 mb-4">
        <svg class="w-8 h-8 text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="10"/>
          <line x1="2" y1="12" x2="22" y2="12"/>
          <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
        </svg>
        <h2 class="text-lg font-semibold">Pair browser extension</h2>
      </div>

      <p class="text-sm text-muted-foreground mb-4">
        <span class="font-medium text-foreground">{clientName}</span> wants to connect to NeuroSkill
        and start tracking browsing activity for EEG correlation.
      </p>

      <div class="bg-muted dark:bg-white/[0.04] rounded-lg p-3 text-xs space-y-1 mb-5">
        <div>• Domain & content type tracking (privacy-respecting)</div>
        <div>• Brain state correlation (focus, distraction)</div>
        <div>• You can revoke access anytime in Settings → Extensions</div>
      </div>

      <div class="flex gap-2 justify-end">
        <button
          onclick={deny}
          disabled={approving}
          class="px-4 py-2 text-sm rounded-lg border border-border hover:bg-muted transition-colors disabled:opacity-50"
        >
          Deny
        </button>
        <button
          onclick={approve}
          disabled={approving}
          class="px-4 py-2 text-sm rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
        >
          {approving ? "Approving…" : "Approve"}
        </button>
      </div>
    </div>
  </div>
{/if}
