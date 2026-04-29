// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! System keychain helpers for storing secrets securely.
//!
//! Uses the OS credential store via the `keyring` crate:
//! - **macOS**: Keychain Services
//! - **Linux**: Secret Service (GNOME Keyring / KWallet)
//! - **Windows**: Windows Credential Manager
//!
//! Secrets survive app re-installs and build updates because they live in
//! the system credential store, not in the app data directory.

#[cfg(not(debug_assertions))]
use keyring::Entry;

/// Service name used as the keychain namespace for all NeuroSkill secrets.
#[cfg(not(debug_assertions))]
const SERVICE: &str = "com.neuroskill.skill";

// ── Debug-build in-memory store ──────────────────────────────────────────────
//
// Debug builds (`cargo run`, `tauri dev`, `cargo test`) deliberately avoid the
// OS keychain — every rebuild produces a binary with a different code
// signature, which on macOS triggers a fresh authorization prompt. The dev
// loop becomes unbearable.
//
// Pre-this commit the workaround was to short-circuit getters to `""` and
// setters to no-op, but that broke any code (including unit tests) that
// expected `set` then `get` to roundtrip. We now keep a process-local
// `Mutex<HashMap>` instead — no OS prompt, but values survive within the same
// process so the route handlers behave like real keychain code.
//
// Release builds bypass this entirely and use `keyring::Entry`.

#[cfg(debug_assertions)]
mod dev_store {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::OnceLock;

    static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

    fn store() -> &'static Mutex<HashMap<String, String>> {
        STORE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub fn get(key: &str) -> String {
        store()
            .lock()
            .ok()
            .and_then(|g| g.get(key).cloned())
            .unwrap_or_default()
    }

    pub fn set(key: &str, value: &str) {
        if let Ok(mut g) = store().lock() {
            if value.is_empty() {
                g.remove(key);
            } else {
                g.insert(key.to_string(), value.to_string());
            }
        }
    }
}

// ── Key names ─────────────────────────────────────────────────────────────────

const KEY_API_TOKEN: &str = "api_token";
const KEY_EMOTIV_CLIENT_ID: &str = "emotiv_client_id";
const KEY_EMOTIV_CLIENT_SECRET: &str = "emotiv_client_secret";
const KEY_IDUN_API_TOKEN: &str = "idun_api_token";
const KEY_OURA_ACCESS_TOKEN: &str = "oura_access_token";
const KEY_NEUROSITY_EMAIL: &str = "neurosity_email";
const KEY_NEUROSITY_PASSWORD: &str = "neurosity_password";
const KEY_NEUROSITY_DEVICE_ID: &str = "neurosity_device_id";

// ── Low-level helpers ─────────────────────────────────────────────────────────
//
// In debug builds these route through `dev_store` (process-local, no OS
// keychain access). In release they hit the real OS keychain. Per-secret
// helpers above don't need their own `cfg!(debug_assertions)` checks — the
// switch happens here so the callers behave identically in both modes.

#[cfg(not(debug_assertions))]
fn get_secret(key: &str) -> String {
    match Entry::new(SERVICE, key).and_then(|e| e.get_password()) {
        Ok(v) => v,
        Err(keyring::Error::NoEntry) => String::new(),
        Err(e) => {
            eprintln!("[keychain] failed to read {key}: {e}");
            String::new()
        }
    }
}

#[cfg(debug_assertions)]
fn get_secret(key: &str) -> String {
    dev_store::get(key)
}

#[cfg(not(debug_assertions))]
fn set_secret(key: &str, value: &str) {
    let entry = match Entry::new(SERVICE, key) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("[keychain] failed to create entry for {key}: {e}");
            return;
        }
    };
    if value.is_empty() {
        // Remove the entry when the value is cleared.
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => eprintln!("[keychain] failed to delete {key}: {e}"),
        }
    } else if let Err(e) = entry.set_password(value) {
        eprintln!("[keychain] failed to store {key}: {e}");
    }
}

#[cfg(debug_assertions)]
fn set_secret(key: &str, value: &str) {
    dev_store::set(key, value);
}

// ── Public API ────────────────────────────────────────────────────────────────

/// All secret fields managed by the keychain.
#[derive(Clone, Debug, Default)]
pub struct Secrets {
    pub api_token: String,
    pub emotiv_client_id: String,
    pub emotiv_client_secret: String,
    pub idun_api_token: String,
    pub oura_access_token: String,
    pub neurosity_email: String,
    pub neurosity_password: String,
    pub neurosity_device_id: String,
}

// ── Lazy per-secret accessors ─────────────────────────────────────────────────
//
// macOS prompts for keychain access whenever the calling binary's code
// signature doesn't match the ACL on a stored item.  A fresh app build has
// a fresh signature, so eagerly reading every secret at startup produces
// one prompt per item per process, before the user has done anything.
//
// These accessors read individual entries on demand, so the OS keychain
// prompt only appears when the user initiates an action that actually needs
// the secret (e.g. clicking "Connect Emotiv" or opening the device settings
// tab). In debug builds the low-level helpers route through `dev_store`
// instead of the OS keychain, so dev/test workflows roundtrip values without
// any auth dialogs.

pub fn get_api_token() -> String {
    get_secret(KEY_API_TOKEN)
}

pub fn set_api_token(value: &str) {
    set_secret(KEY_API_TOKEN, value);
}

pub fn get_emotiv_credentials() -> (String, String) {
    (get_secret(KEY_EMOTIV_CLIENT_ID), get_secret(KEY_EMOTIV_CLIENT_SECRET))
}

pub fn get_idun_api_token() -> String {
    get_secret(KEY_IDUN_API_TOKEN)
}

pub fn get_oura_access_token() -> String {
    get_secret(KEY_OURA_ACCESS_TOKEN)
}

pub fn get_neurosity_credentials() -> (String, String, String) {
    (
        get_secret(KEY_NEUROSITY_EMAIL),
        get_secret(KEY_NEUROSITY_PASSWORD),
        get_secret(KEY_NEUROSITY_DEVICE_ID),
    )
}

pub fn get_neurosity_device_id() -> String {
    get_secret(KEY_NEUROSITY_DEVICE_ID)
}

/// Write device-API secrets supplied in `secrets` to the keychain.
///
/// Empty fields are **ignored** rather than treated as deletion: if the user
/// denies a keychain prompt during the GET round-trip, the in-memory copy of
/// untouched secrets will be empty, and we don't want to clobber valid stored
/// values on the next save.  Use [`set_api_token`] (or extend with explicit
/// delete helpers) when an empty value is genuinely meant to clear.
///
/// Used by the daemon's `set_device_api_config` route.
pub fn save_device_api_secrets(secrets: &Secrets) {
    let pairs: &[(&str, &str)] = &[
        (KEY_EMOTIV_CLIENT_ID, &secrets.emotiv_client_id),
        (KEY_EMOTIV_CLIENT_SECRET, &secrets.emotiv_client_secret),
        (KEY_IDUN_API_TOKEN, &secrets.idun_api_token),
        (KEY_OURA_ACCESS_TOKEN, &secrets.oura_access_token),
        (KEY_NEUROSITY_EMAIL, &secrets.neurosity_email),
        (KEY_NEUROSITY_PASSWORD, &secrets.neurosity_password),
        (KEY_NEUROSITY_DEVICE_ID, &secrets.neurosity_device_id),
    ];
    for &(key, value) in pairs {
        if !value.is_empty() {
            set_secret(key, value);
        }
    }
}

/// Load all secrets eagerly from the keychain.
///
/// Retained only for the legacy round-trip through [`save_secrets`] used by
/// the Tauri shell's `save_settings_now`.  New code should use the per-secret
/// accessors above so prompts only fire on user-initiated actions.
pub fn load_secrets() -> Secrets {
    Secrets {
        api_token: get_secret(KEY_API_TOKEN),
        emotiv_client_id: get_secret(KEY_EMOTIV_CLIENT_ID),
        emotiv_client_secret: get_secret(KEY_EMOTIV_CLIENT_SECRET),
        idun_api_token: get_secret(KEY_IDUN_API_TOKEN),
        oura_access_token: get_secret(KEY_OURA_ACCESS_TOKEN),
        neurosity_email: get_secret(KEY_NEUROSITY_EMAIL),
        neurosity_password: get_secret(KEY_NEUROSITY_PASSWORD),
        neurosity_device_id: get_secret(KEY_NEUROSITY_DEVICE_ID),
    }
}

/// Save all secrets to the system keychain.
///
/// Empty values are **ignored** rather than treated as a deletion request.
/// This avoids clobbering previously-stored secrets when the caller's
/// in-memory copy was never populated (e.g. lazy-load callers that don't
/// hydrate every field).  Use the dedicated `set_*` helpers above to
/// explicitly delete a secret.
///
pub fn save_secrets(secrets: &Secrets) {
    let pairs: &[(&str, &str)] = &[
        (KEY_API_TOKEN, &secrets.api_token),
        (KEY_EMOTIV_CLIENT_ID, &secrets.emotiv_client_id),
        (KEY_EMOTIV_CLIENT_SECRET, &secrets.emotiv_client_secret),
        (KEY_IDUN_API_TOKEN, &secrets.idun_api_token),
        (KEY_OURA_ACCESS_TOKEN, &secrets.oura_access_token),
        (KEY_NEUROSITY_EMAIL, &secrets.neurosity_email),
        (KEY_NEUROSITY_PASSWORD, &secrets.neurosity_password),
        (KEY_NEUROSITY_DEVICE_ID, &secrets.neurosity_device_id),
    ];
    for &(key, value) in pairs {
        if !value.is_empty() {
            set_secret(key, value);
        }
    }
}

/// Migrate plaintext secrets from settings JSON into the keychain.
///
/// Called once during `load_settings`.  If the JSON still contains non-empty
/// secret values **and** the keychain entry is empty, the value is copied
/// into the keychain.  Returns `true` if any migration happened (caller
/// should re-save settings to strip the plaintext values).
pub fn migrate_plaintext_secrets(secrets: &Secrets) -> bool {
    let mut migrated = false;

    let pairs: &[(&str, &str)] = &[
        (KEY_API_TOKEN, &secrets.api_token),
        (KEY_EMOTIV_CLIENT_ID, &secrets.emotiv_client_id),
        (KEY_EMOTIV_CLIENT_SECRET, &secrets.emotiv_client_secret),
        (KEY_IDUN_API_TOKEN, &secrets.idun_api_token),
        (KEY_OURA_ACCESS_TOKEN, &secrets.oura_access_token),
        (KEY_NEUROSITY_EMAIL, &secrets.neurosity_email),
        (KEY_NEUROSITY_PASSWORD, &secrets.neurosity_password),
        (KEY_NEUROSITY_DEVICE_ID, &secrets.neurosity_device_id),
    ];

    for &(key, plaintext) in pairs {
        if !plaintext.is_empty() && get_secret(key).is_empty() {
            set_secret(key, plaintext);
            migrated = true;
        }
    }

    migrated
}

#[cfg(test)]
mod tests {
    use super::*;

    // Keychain tests are inherently platform-specific and may fail in CI
    // containers that lack a credential store.  We only verify the API
    // compiles and the round-trip works when a store is available.

    #[test]
    fn round_trip_secret() {
        let key = "skill_test_round_trip";
        // Clean up from previous runs.
        set_secret(key, "");

        assert!(get_secret(key).is_empty());

        set_secret(key, "hello-world");
        let got = get_secret(key);
        // Clean up.
        set_secret(key, "");

        // If the platform has no credential store the set may silently fail.
        if !got.is_empty() {
            assert_eq!(got, "hello-world");
        }
    }
}
