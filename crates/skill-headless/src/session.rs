// SPDX-License-Identifier: GPL-3.0-only
//! Cookie and storage data types.

use serde::{Deserialize, Serialize};

/// An HTTP cookie (simplified model, mirrors CDP Network.Cookie).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub path: String,
    /// Expiry as Unix timestamp (seconds). 0 = session cookie.
    #[serde(default)]
    pub expires: f64,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub same_site: SameSite,
}

/// SameSite cookie attribute.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum SameSite {
    #[default]
    None,
    Lax,
    Strict,
}

impl SameSite {
    pub fn as_str(&self) -> &str {
        match self {
            Self::None => "None",
            Self::Lax => "Lax",
            Self::Strict => "Strict",
        }
    }
}

/// A key-value pair from localStorage or sessionStorage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEntry {
    pub key: String,
    pub value: Option<String>,
}
