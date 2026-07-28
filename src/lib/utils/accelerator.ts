// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.

/**
 * Pretty-print a Tauri accelerator string for display.
 * "CmdOrCtrl+Shift+L" → "⌘⇧L" on Mac, "Ctrl+Shift+L" elsewhere.
 */
export function prettyAccelerator(accel: string): string {
  if (!accel) return "—";
  const isMac =
    typeof navigator !== "undefined" && (navigator.platform?.startsWith("Mac") || navigator.userAgent.includes("Mac"));
  let s = accel;
  if (isMac) {
    s = s
      .replace(/CmdOrCtrl/gi, "⌘")
      .replace(/CommandOrControl/gi, "⌘")
      .replace(/Ctrl/gi, "⌃")
      .replace(/Cmd/gi, "⌘")
      .replace(/Alt/gi, "⌥")
      .replace(/Shift/gi, "⇧")
      .replace(/\+/g, "");
  } else {
    s = s.replace(/CmdOrCtrl/gi, "Ctrl").replace(/CommandOrControl/gi, "Ctrl");
  }
  return s;
}
