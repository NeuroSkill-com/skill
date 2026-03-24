<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  WhatsNew — startup trigger that opens the What's New standalone window.

  Fetches the last-seen version from settings.json (via the Rust backend) and
  compares it to the running version.  If they differ it opens the dedicated
  "whats-new" window.  The window's own page calls dismiss_whats_new on
  dismiss, which persists the version in skill_dir/settings.json and closes
  the window from the Rust side.

  This component renders nothing visible.
-->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onMount } from "svelte";

onMount(async () => {
  try {
    if (getCurrentWindow().label !== "main") return;
    const [appVersion, seenVersion] = await Promise.all([
      invoke<string>("get_app_version"),
      invoke<string>("get_whats_new_seen_version"),
    ]);
    if (seenVersion !== appVersion) {
      await invoke("open_whats_new_window");
    }
  } catch {
    /* fail silently — don't block startup */
  }
});
</script>
