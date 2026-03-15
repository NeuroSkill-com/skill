// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Shared reactive state between label/+page.svelte (writes) and
// CustomTitleBar.svelte (reads). Keeps the live EEG window timer in the
// shared titlebar instead of an extra in-page header row.

import { createTitlebarState } from "$lib/titlebar-state.svelte";

export const labelTitlebarState = createTitlebarState({ active: false, elapsed: "0s" });
