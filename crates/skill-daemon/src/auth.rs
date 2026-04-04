// SPDX-License-Identifier: GPL-3.0-only
//! Multi-token authentication with names, ACLs, and expiration.

use serde::{Deserialize, Serialize};
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
        match self {
            Self::Admin => true,
            Self::ReadOnly => method == "GET",
            Self::Data => {
                method == "GET"
                    || path.starts_with("/v1/labels")
                    || path.starts_with("/v1/history")
                    || path.starts_with("/v1/search")
                    || path.starts_with("/v1/analysis")
                    || path.starts_with("/v1/llm/chat")
            }
            Self::Stream => {
                path.starts_with("/v1/events")
                    || path.starts_with("/v1/status")
                    || path == "/v1/version"
                    || path.starts_with("/v1/activity/latest-bands")
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

    pub fn label(&self) -> &'static str {
        match self {
            Self::Week => "1 week",
            Self::Month => "30 days",
            Self::Quarter => "90 days",
            Self::Never => "never",
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
    /// The secret token string (shown once at creation, then redacted).
    pub token: String,
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

fn generate_id() -> String {
    let mut bytes = [0u8; 8];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn store_path(skill_dir: &Path) -> PathBuf {
    skill_dir.join("daemon").join("tokens.json")
}

impl TokenStore {
    pub fn load(skill_dir: &Path) -> Self {
        let path = store_path(skill_dir);
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
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

    /// Create a new token. Returns the full token (including secret).
    pub fn create(&mut self, name: String, acl: TokenAcl, expiry: TokenExpiry) -> ApiToken {
        let created_at = now_unix();
        let token = ApiToken {
            id: generate_id(),
            name,
            token: generate_token_secret(),
            acl,
            created_at,
            expires_at: expiry.to_unix(created_at),
            last_used_at: None,
            revoked: false,
        };
        self.tokens.push(token.clone());
        token
    }

    /// Validate a token string. Returns the token if valid, updates last_used_at.
    pub fn validate(&mut self, secret: &str) -> Option<&ApiToken> {
        let now = now_unix();
        let idx = self.tokens.iter().position(|t| t.token == secret && t.is_valid())?;
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

    /// List all tokens (secrets redacted for display).
    pub fn list_redacted(&self) -> Vec<ApiToken> {
        self.tokens
            .iter()
            .map(|t| {
                let mut redacted = t.clone();
                if redacted.token.len() > 10 {
                    redacted.token = format!("{}…{}", &t.token[..6], &t.token[t.token.len() - 4..]);
                }
                redacted
            })
            .collect()
    }
}
