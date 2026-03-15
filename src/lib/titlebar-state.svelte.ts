// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Factory for reactive titlebar state shared between page components (writers)
// and CustomTitleBar.svelte (reader).  Each page creates its own store with
// page-specific data + an `active` flag that CustomTitleBar uses to decide
// which controls to render.

/**
 * Create a module-level `$state` store with a typed initial value.
 *
 * The returned proxy is readable and writable from any module that imports it
 * (Svelte 5 module-level `$state`).
 *
 * ```ts
 * // chat-titlebar.svelte.ts
 * export const chatTitlebar = createTitlebarState({ modelName: "", status: "stopped" as const });
 *
 * // page writes:   chatTitlebar.modelName = "phi-3";
 * // titlebar reads: {chatTitlebar.modelName}
 * ```
 */
export function createTitlebarState<T extends Record<string, unknown>>(initial: T): T {
  return $state(initial) as T;
}

/**
 * Create a callback bag initialised with no-op functions.
 *
 * ```ts
 * export const hCbs = createTitlebarCallbacks({ prev: () => {}, next: () => {} });
 * // page sets:      hCbs.prev = () => navigatePrev();
 * // titlebar calls: hCbs.prev();
 * ```
 */
export function createTitlebarCallbacks<T extends Record<string, (...args: any[]) => void>>(
  defaults: T,
): T {
  // Plain object — intentionally not reactive.  The titlebar calls these
  // synchronously; Svelte reactivity is not needed on the callback bag itself.
  return { ...defaults };
}
