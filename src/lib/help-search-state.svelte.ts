// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Shared reactive state between help/+page.svelte (writes) and
 * CustomTitleBar.svelte (reads + binds).  Allows the search input and
 * version badge to live in the custom titlebar while the filtering logic
 * stays in the help page.
 */

export const helpTitlebarState = $state({ query: "", version: "…" });
