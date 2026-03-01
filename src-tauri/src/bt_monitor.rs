// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/// Returns `true` when the system Bluetooth radio is powered on.
///
/// This is a direct OS-level read that does not go through btleplug at all,
/// so it works even before a `BtPlatformManager` is created, and it responds
/// within a single poll interval (~500 ms) when the radio changes state.
/// Calling it many times per second is safe — each call is a single integer
/// read or tiny filesystem read with no side effects.
pub fn bt_is_on() -> bool {
    platform::bt_is_on()
}

// ── macOS ──────────────────────────────────────────────────────────────────────
//
// IOBluetoothPreferenceGetControllerPowerState() is part of the public
// IOBluetooth.framework surface.  It returns 1 when the radio is on and 0
// when it is off (or the adapter is absent).

#[cfg(target_os = "macos")]
mod platform {
    #[link(name = "IOBluetooth", kind = "framework")]
    extern "C" {
        fn IOBluetoothPreferenceGetControllerPowerState() -> std::os::raw::c_int;
    }

    pub fn bt_is_on() -> bool {
        // SAFETY: no pointers, no state — pure read of a kernel register.
        unsafe { IOBluetoothPreferenceGetControllerPowerState() == 1 }
    }
}

// ── Linux ──────────────────────────────────────────────────────────────────────
//
// Walk /sys/class/rfkill/ to find the Bluetooth rfkill entry.
// The `state` file contains: "0" = soft-blocked (off), "1" = unblocked (on),
// "2" = hard-blocked (off).  Multiple BT adapters are unusual; we take the
// first one found.

#[cfg(target_os = "linux")]
mod platform {
    pub fn bt_is_on() -> bool {
        let Ok(dir) = std::fs::read_dir("/sys/class/rfkill") else {
            return true; // no rfkill subsystem — assume adapter present
        };
        for entry in dir.flatten() {
            let path = entry.path();
            let is_bt = std::fs::read_to_string(path.join("type"))
                .map_or(false, |t| t.trim() == "bluetooth");
            if is_bt {
                // "1" = unblocked = on
                return std::fs::read_to_string(path.join("state"))
                    .map_or(true, |s| s.trim() == "1");
            }
        }
        true // no BT rfkill entry — assume adapter present and on
    }
}

// ── Other platforms (Windows …) ────────────────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod platform {
    pub fn bt_is_on() -> bool {
        true // conservative: assume on; btleplug will surface the real error
    }
}
