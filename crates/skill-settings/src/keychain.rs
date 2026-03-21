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

use keyring::Entry;

/// Service name used as the keychain namespace for all NeuroSkill secrets.
const SERVICE: &str = "com.neuroskill.skill";

// ── Key names ─────────────────────────────────────────────────────────────────

const KEY_API_TOKEN: &str = "api_token";
const KEY_EMOTIV_CLIENT_ID: &str = "emotiv_client_id";
const KEY_EMOTIV_CLIENT_SECRET: &str = "emotiv_client_secret";
const KEY_IDUN_API_TOKEN: &str = "idun_api_token";

// ── Low-level helpers ─────────────────────────────────────────────────────────

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
    } else {
        if let Err(e) = entry.set_password(value) {
            eprintln!("[keychain] failed to store {key}: {e}");
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// All secret fields managed by the keychain.
#[derive(Clone, Debug, Default)]
pub struct Secrets {
    pub api_token: String,
    pub emotiv_client_id: String,
    pub emotiv_client_secret: String,
    pub idun_api_token: String,
}

/// Load all secrets from the system keychain.
pub fn load_secrets() -> Secrets {
    Secrets {
        api_token:            get_secret(KEY_API_TOKEN),
        emotiv_client_id:     get_secret(KEY_EMOTIV_CLIENT_ID),
        emotiv_client_secret: get_secret(KEY_EMOTIV_CLIENT_SECRET),
        idun_api_token:       get_secret(KEY_IDUN_API_TOKEN),
    }
}

/// Save all secrets to the system keychain.
pub fn save_secrets(secrets: &Secrets) {
    set_secret(KEY_API_TOKEN,            &secrets.api_token);
    set_secret(KEY_EMOTIV_CLIENT_ID,     &secrets.emotiv_client_id);
    set_secret(KEY_EMOTIV_CLIENT_SECRET, &secrets.emotiv_client_secret);
    set_secret(KEY_IDUN_API_TOKEN,       &secrets.idun_api_token);
}

/// Migrate plaintext secrets from settings JSON into the keychain.
///
/// Called once during `load_settings`.  If the JSON still contains non-empty
/// secret values **and** the keychain entry is empty, the value is copied
/// into the keychain.  Returns `true` if any migration happened (caller
/// should re-save settings to strip the plaintext values).
pub fn migrate_plaintext_secrets(
    api_token: &str,
    emotiv_client_id: &str,
    emotiv_client_secret: &str,
    idun_api_token: &str,
) -> bool {
    let mut migrated = false;

    let pairs: &[(&str, &str)] = &[
        (KEY_API_TOKEN,            api_token),
        (KEY_EMOTIV_CLIENT_ID,     emotiv_client_id),
        (KEY_EMOTIV_CLIENT_SECRET, emotiv_client_secret),
        (KEY_IDUN_API_TOKEN,       idun_api_token),
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
