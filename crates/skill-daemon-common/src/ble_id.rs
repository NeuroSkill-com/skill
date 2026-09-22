// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Canonical spelling for BLE device identifiers.
//!
//! The daemon persists device ids as `ble:<id>` in `paired_devices.json` and
//! then compares them by exact string equality in a dozen places, so the
//! spelling a BLE backend happens to hand us is load-bearing.  The two
//! backends disagree about it:
//!
//! |              | macOS                                | Linux / Windows              |
//! |--------------|--------------------------------------|------------------------------|
//! | btleplug     | `Uuid: Display` — **lowercase**      | `BDAddr: UpperHex` — uppercase |
//! | webbluetooth | `NSUUID.UUIDString` — **UPPERCASE**  | address — uppercase          |
//!
//! Migrating the scanner to webbluetooth therefore flips every macOS id from
//! `a1b2…` to `A1B2…`, and every headset the user paired before the upgrade
//! silently stops matching — it reappears in the scan list as unpaired and
//! auto-reconnect never fires.
//!
//! One rule fixes it: **a BLE id is lowercase**.  That is byte-for-byte
//! btleplug's macOS spelling, so Apple pairings survive the upgrade untouched;
//! Linux and Windows ids were uppercase under both backends, so those need the
//! one-time [`canonical_target`] pass that [`crate`]'s callers apply when
//! loading the persisted list.
//!
//! Only `ble:` targets are touched.  Other transports share this id space and
//! are case-sensitive for real — a Windows serial target is `usb:COM3`, and
//! lowercasing it to `usb:com3` would break a device that was working.

/// The transport prefix this module owns.
const BLE_PREFIX: &str = "ble:";

/// Canonicalise a bare BLE identifier — the part *after* `ble:`.
///
/// ```
/// use skill_daemon_common::ble_id::canonical;
/// assert_eq!(canonical("A1B2C3D4-0000-1111-2222-333344445555"),
///            "a1b2c3d4-0000-1111-2222-333344445555");
/// ```
pub fn canonical(id: &str) -> String {
    id.to_ascii_lowercase()
}

/// Canonicalise a device target, leaving non-BLE transports alone.
///
/// Recognises the prefix case-insensitively so a target that already went
/// through a differently-cased code path still normalises, and always emits
/// the lowercase `ble:` form.
///
/// ```
/// use skill_daemon_common::ble_id::canonical_target;
/// assert_eq!(canonical_target("ble:AA:BB:CC:DD:EE:FF"), "ble:aa:bb:cc:dd:ee:ff");
/// assert_eq!(canonical_target("usb:COM3"), "usb:COM3");
/// ```
pub fn canonical_target(target: &str) -> String {
    match split_ble(target) {
        Some(id) => format!("{BLE_PREFIX}{}", canonical(id)),
        None => target.to_owned(),
    }
}

/// Whether two device targets name the same device.
///
/// Case-insensitive for `ble:` targets on both sides, exact for everything
/// else.  Prefer this to `==` at comparison sites: it keeps working against a
/// `paired_devices.json` written by an older build, whichever case that build
/// happened to store, without depending on a migration having run first.
///
/// ```
/// use skill_daemon_common::ble_id::same_device;
/// assert!(same_device("ble:AB-CD", "ble:ab-cd"));
/// assert!(!same_device("usb:COM3", "usb:com3"));
/// assert!(!same_device("ble:ab-cd", "cortex:ab-cd"));
/// ```
pub fn same_device(a: &str, b: &str) -> bool {
    match (split_ble(a), split_ble(b)) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        // One side is BLE and the other is not: different transports, and
        // never the same device even if the remainder happens to match.
        (Some(_), None) | (None, Some(_)) => false,
        (None, None) => a == b,
    }
}

/// The identifier part of a `ble:` target, or `None` for other transports.
fn split_ble(target: &str) -> Option<&str> {
    let rest = target.get(..BLE_PREFIX.len())?;
    rest.eq_ignore_ascii_case(BLE_PREFIX)
        .then(|| &target[BLE_PREFIX.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_apple_uuid_to_the_btleplug_spelling() {
        // webbluetooth reports NSUUID.UUIDString (uppercase); the pre-migration
        // cache keys and paired_devices.json entries are btleplug's lowercase
        // Uuid Display.  Canonicalising the former must reproduce the latter.
        let webbluetooth = "ble:A1B2C3D4-1111-2222-3333-444455556666";
        let btleplug = "ble:a1b2c3d4-1111-2222-3333-444455556666";
        assert_eq!(canonical_target(webbluetooth), btleplug);
        assert_eq!(canonical_target(btleplug), btleplug);
    }

    #[test]
    fn lowercases_linux_addresses() {
        assert_eq!(canonical_target("ble:00:1A:7D:DA:71:13"), "ble:00:1a:7d:da:71:13");
    }

    #[test]
    fn canonical_target_is_idempotent() {
        for t in ["ble:AB:CD", "ble:ab:cd", "usb:COM3", "cortex:EPOCX-1234", "muse", ""] {
            let once = canonical_target(t);
            assert_eq!(canonical_target(&once), once, "not idempotent for {t:?}");
        }
    }

    #[test]
    fn leaves_other_transports_alone() {
        // Windows serial targets are genuinely case-sensitive.
        for t in ["usb:COM3", "cortex:EPOCX-1234", "peer:AbC", "lsl", "BLEACH"] {
            assert_eq!(canonical_target(t), t);
        }
    }

    #[test]
    fn recognises_the_prefix_case_insensitively() {
        assert_eq!(canonical_target("BLE:AB-CD"), "ble:ab-cd");
        // …but a name that merely starts with those letters is not a prefix.
        assert_eq!(canonical_target("BLEACH-9"), "BLEACH-9");
    }

    #[test]
    fn same_device_is_case_insensitive_only_within_ble() {
        assert!(same_device("ble:AB-CD", "ble:ab-cd"));
        assert!(same_device("ble:ab-cd", "ble:ab-cd"));
        assert!(!same_device("ble:ab-cd", "ble:ab-ce"));
        assert!(!same_device("usb:COM3", "usb:com3"));
        assert!(same_device("usb:COM3", "usb:COM3"));
    }

    #[test]
    fn same_device_never_crosses_transports() {
        assert!(!same_device("ble:ab-cd", "ab-cd"));
        assert!(!same_device("ab-cd", "ble:ab-cd"));
        assert!(!same_device("ble:ab-cd", "cortex:ab-cd"));
    }

    #[test]
    fn split_ble_does_not_panic_on_short_or_multibyte_input() {
        // get(..4) returns None on a short string and on a char-boundary miss,
        // where slicing would panic.
        for t in ["", "b", "ble", "ble:", "日本語", "b日:x"] {
            let _ = canonical_target(t);
            let _ = same_device(t, "ble:x");
        }
        assert_eq!(split_ble("ble:"), Some(""));
        assert_eq!(split_ble("ble"), None);
    }
}
