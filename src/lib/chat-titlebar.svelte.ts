// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Shared reactive state between chat/+page.svelte (writes) and
// CustomTitleBar.svelte (reads). Shows the active model name + status in the titlebar.

export type LlmStatus = "stopped" | "loading" | "running";

export const chatTitlebarState = $state<{
  modelName: string;
  status: LlmStatus;
}>({ modelName: "", status: "stopped" });
