// SPDX-License-Identifier: GPL-3.0-only
//! Multi-token authentication with names, ACLs, and expiration.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use rand::RngCore;

/// Access control level for a token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TokenAcl {
    /// Full access — all endpoints.
    Admin,
    /// Read-only — GET endpoints only, no mutations.
    ReadOnly,
    /// Data access — labels, history, search, analysis. No settings/control.
    Data,
    /// Streaming only — EEG/events WebSocket, status. No mutations.
    Stream,
}

impl std::fmt::Display for TokenAcl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admin => write!(f, "admin"),
            Self::ReadOnly => write!(f, "read_only"),
            Self::Data => write!(f, "data"),
            Self::Stream => write!(f, "stream"),
        }
    }
}

impl TokenAcl {
    /// Check if this ACL permits the given HTTP method + path.
    pub fn allows(&self, method: &str, path: &str) -> bool {
        let method = method.to_ascii_uppercase();
        let is_read = matches!(method.as_str(), "GET" | "HEAD");

        match self {
            Self::Admin => true,
            Self::ReadOnly => is_read,
            Self::Data => {
                // Data-scope tokens can use data namespaces only.
                // This intentionally excludes auth/control/settings/admin routes.
                path.starts_with("/v1/labels")
                    || path.starts_with("/v1/history")
                    || path.starts_with("/v1/search")
                    || path.starts_with("/v1/analysis")
                    || path.starts_with("/v1/llm/chat")
                    || (is_read
                        && (path == "/v1/version"
                            || path.starts_with("/v1/status")
                            || path.starts_with("/v1/activity/latest-bands")))
            }
            Self::Stream => {
                // Stream tokens are read-only and limited to live status/event feeds.
                is_read
                    && (path.starts_with("/v1/events")
                        || path.starts_with("/v1/status")
                        || path == "/v1/version"
                        || path.starts_with("/v1/activity/latest-bands"))
            }
        }
    }
}

/// Expiration duration for a token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TokenExpiry {
    /// 7 days.
    Week,
    /// 30 days.
    Month,
    /// 90 days.
    Quarter,
    /// Never expires.
    Never,
}

impl TokenExpiry {
    pub fn to_unix(&self, created_at: u64) -> Option<u64> {
        match self {
            Self::Week => Some(created_at + 7 * 86400),
            Self::Month => Some(created_at + 30 * 86400),
            Self::Quarter => Some(created_at + 90 * 86400),
            Self::Never => None,
        }
    }
}

/// A single API token with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToken {
    /// Unique identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Secret token string.
    /// - Returned once at creation.
    /// - For persisted store entries this is intentionally blank.
    #[serde(default)]
    pub token: String,
    /// SHA-256(secret + per-token salt) hex digest.
    #[serde(default)]
    pub token_hash: String,
    /// Random per-token salt used for hashing.
    #[serde(default)]
    pub token_salt: String,
    /// Non-sensitive preview shown in token list.
    #[serde(default)]
    pub token_preview: String,
    /// Access control level.
    pub acl: TokenAcl,
    /// Unix timestamp of creation.
    pub created_at: u64,
    /// Unix timestamp of expiration, or None for never.
    pub expires_at: Option<u64>,
    /// Unix timestamp of last use, or None if never used.
    pub last_used_at: Option<u64>,
    /// Whether the token has been revoked.
    pub revoked: bool,
}

impl ApiToken {
    pub fn is_expired(&self) -> bool {
        if let Some(exp) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            now > exp
        } else {
            false
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.revoked && !self.is_expired()
    }

    fn for_api_response(mut self) -> Self {
        self.token_hash.clear();
        self.token_salt.clear();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenStoreError {
    MaxTokensReached,
}

impl std::fmt::Display for TokenStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MaxTokensReached => write!(f, "maximum number of active tokens reached"),
        }
    }
}

/// Token store — persisted as JSON in the skill data dir.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenStore {
    pub tokens: Vec<ApiToken>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn generate_token_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    format!("sk-{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn generate_token_salt() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_secret(secret: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(secret.as_bytes());
    hex::encode(hasher.finalize())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let max_len = a.len().max(b.len());
    let mut diff = a.len() ^ b.len();

    for i in 0..max_len {
        let av = *a.get(i).unwrap_or(&0);
        let bv = *b.get(i).unwrap_or(&0);
        diff |= usize::from(av ^ bv);
    }

    diff == 0
}

fn token_preview(secret: &str) -> String {
    if secret.len() > 10 {
        format!("{}…{}", &secret[..6], &secret[secret.len() - 4..])
    } else {
        "hidden".to_string()
    }
}

fn store_path(skill_dir: &Path) -> PathBuf {
    skill_dir.join("daemon").join("tokens.json")
}

impl TokenStore {
    /// Maximum active tokens allowed.
    pub const MAX_TOKENS: usize = 50;

    pub fn load(skill_dir: &Path) -> Self {
        let path = store_path(skill_dir);
        if let Ok(data) = std::fs::read_to_string(&path) {
            let mut store: Self = serde_json::from_str(&data).unwrap_or_default();
            let mut migrated = false;

            // One-time migration: legacy plaintext tokens -> salted hashes.
            for t in &mut store.tokens {
                if t.token_hash.is_empty() && !t.token.is_empty() {
                    t.token_preview = token_preview(&t.token);
                    t.token_salt = generate_token_salt();
                    t.token_hash = hash_secret(&t.token, &t.token_salt);
                    t.token.clear();
                    migrated = true;
                } else if t.token_preview.is_empty() {
                    t.token_preview = if !t.token.is_empty() {
                        token_preview(&t.token)
                    } else if !t.token_hash.is_empty() {
                        format!("hash:{}", &t.token_hash[..8.min(t.token_hash.len())])
                    } else {
                        "hidden".to_string()
                    };
                    migrated = true;
                }
            }

            if migrated {
                let _ = store.save(skill_dir);
            }

            store
        } else {
            Self::default()
        }
    }

    pub fn save(&self, skill_dir: &Path) -> Result<(), String> {
        let path = store_path(skill_dir);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| format!("serialize error: {e}"))?;
        std::fs::write(&path, json).map_err(|e| format!("write error: {e}"))
    }

    /// Create a new token. Returns the full token only once.
    pub fn create(&mut self, name: String, acl: TokenAcl, expiry: TokenExpiry) -> Result<ApiToken, TokenStoreError> {
        let active_count = self.tokens.iter().filter(|t| t.is_valid()).count();
        if active_count >= Self::MAX_TOKENS {
            return Err(TokenStoreError::MaxTokensReached);
        }

        let created_at = now_unix();
        let secret = generate_token_secret();
        let salt = generate_token_salt();
        let digest = hash_secret(&secret, &salt);
        let preview = token_preview(&secret);

        let stored = ApiToken {
            id: generate_id(),
            name,
            token: String::new(),
            token_hash: digest,
            token_salt: salt,
            token_preview: preview,
            acl,
            created_at,
            expires_at: expiry.to_unix(created_at),
            last_used_at: None,
            revoked: false,
        };

        self.tokens.push(stored.clone());

        let mut created = stored.for_api_response();
        created.token = secret;
        Ok(created)
    }

    /// Validate a token string. Returns the token if valid, updates last_used_at.
    pub fn validate(&mut self, secret: &str) -> Option<&ApiToken> {
        let now = now_unix();
        let idx = self.tokens.iter().position(|t| {
            t.is_valid()
                && if !t.token_hash.is_empty() {
                    let expected = hash_secret(secret, &t.token_salt);
                    constant_time_eq(expected.as_bytes(), t.token_hash.as_bytes())
                } else {
                    // Legacy plaintext fallback.
                    constant_time_eq(secret.as_bytes(), t.token.as_bytes())
                }
        })?;

        self.tokens[idx].last_used_at = Some(now);
        Some(&self.tokens[idx])
    }

    /// Check if a token is valid for a given method + path.
    pub fn authorize(&mut self, secret: &str, method: &str, path: &str) -> bool {
        if let Some(token) = self.validate(secret) {
            token.acl.allows(method, path)
        } else {
            false
        }
    }

    /// Revoke a token by id.
    pub fn revoke(&mut self, id: &str) -> bool {
        if let Some(t) = self.tokens.iter_mut().find(|t| t.id == id) {
            t.revoked = true;
            true
        } else {
            false
        }
    }

    /// Delete a token by id.
    pub fn delete(&mut self, id: &str) -> bool {
        let len = self.tokens.len();
        self.tokens.retain(|t| t.id != id);
        self.tokens.len() < len
    }

    /// List all tokens with secrets hidden.
    pub fn list_redacted(&self) -> Vec<ApiToken> {
        self.tokens
            .iter()
            .cloned()
            .map(|mut t| {
                if !t.token_preview.is_empty() {
                    t.token = t.token_preview.clone();
                } else {
                    t.token = "stored_as_hash".to_string();
                }
                t.for_api_response()
            })
            .collect()
    }
}

fn generate_id() -> String {
    let mut bytes = [0u8; 8];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::{TokenAcl, TokenStore};
    use rand::RngCore;

    #[test]
    fn data_acl_does_not_allow_control_or_auth_routes() {
        assert!(!TokenAcl::Data.allows("POST", "/v1/control/start-session"));
        assert!(!TokenAcl::Data.allows("POST", "/v1/auth/tokens"));
        assert!(!TokenAcl::Data.allows("GET", "/v1/settings/notch-preset"));
    }

    #[test]
    fn data_acl_allows_data_namespaces() {
        assert!(TokenAcl::Data.allows("GET", "/v1/history/sessions"));
        assert!(TokenAcl::Data.allows("POST", "/v1/labels"));
        assert!(TokenAcl::Data.allows("POST", "/v1/analysis/recompute"));
    }

    #[test]
    fn stream_acl_is_read_only() {
        assert!(TokenAcl::Stream.allows("GET", "/v1/events"));
        assert!(TokenAcl::Stream.allows("GET", "/v1/status"));
        assert!(TokenAcl::Stream.allows("GET", "/v1/version"));

        assert!(!TokenAcl::Stream.allows("POST", "/v1/events/push"));
        assert!(!TokenAcl::Stream.allows("POST", "/v1/control/start-session"));
    }

    #[test]
    fn load_migrates_legacy_plaintext_tokens_and_persists() {
        let mut nonce = [0u8; 8];
        rand::rng().fill_bytes(&mut nonce);
        let base = std::env::temp_dir().join(format!("skill-daemon-auth-test-{}", hex::encode(nonce)));
        let daemon_dir = base.join("daemon");
        std::fs::create_dir_all(&daemon_dir).expect("create temp daemon dir");

        let tokens_path = daemon_dir.join("tokens.json");
        let legacy = serde_json::json!({
            "tokens": [{
                "id": "legacy-1",
                "name": "legacy",
                "token": "sk-legacy-secret",
                "acl": "admin",
                "created_at": 0,
                "expires_at": null,
                "last_used_at": null,
                "revoked": false
            }]
        });
        std::fs::write(
            &tokens_path,
            serde_json::to_string_pretty(&legacy).expect("serialize legacy token fixture"),
        )
        .expect("write legacy tokens");

        let store = TokenStore::load(&base);
        assert_eq!(store.tokens.len(), 1);
        let t = &store.tokens[0];
        assert!(t.token.is_empty(), "legacy plaintext token should be cleared");
        assert!(!t.token_hash.is_empty(), "token_hash should be populated");
        assert!(!t.token_salt.is_empty(), "token_salt should be populated");
        assert!(!t.token_preview.is_empty(), "token_preview should be populated");

        let persisted = std::fs::read_to_string(&tokens_path).expect("read persisted tokens");
        let parsed: serde_json::Value = serde_json::from_str(&persisted).expect("parse persisted tokens");
        let p = &parsed["tokens"][0];
        assert_eq!(p["token"], "");
        assert!(p["token_hash"].as_str().is_some_and(|v| !v.is_empty()));
        assert!(p["token_salt"].as_str().is_some_and(|v| !v.is_empty()));

        let _ = std::fs::remove_dir_all(base);
    }
}
