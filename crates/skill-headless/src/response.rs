// SPDX-License-Identifier: GPL-3.0-only
//! Response types returned by the headless browser engine.

use serde::{Deserialize, Serialize};

use crate::session::{Cookie, StorageEntry};

/// Response from a headless browser command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    /// Command completed successfully with no payload.
    Ok,

    /// A text result (HTML, URL, title, JS eval result, attribute value, etc.).
    Text(String),

    /// Multiple text results (e.g. querySelectorAll).
    TextList(Vec<String>),

    /// Binary payload (screenshot PNG, PDF).
    #[serde(with = "base64_bytes")]
    Binary(Vec<u8>),

    /// A list of cookies.
    Cookies(Vec<Cookie>),

    /// A storage entry (localStorage / sessionStorage).
    Storage(StorageEntry),

    /// An error message from the webview.
    Error(String),
}

impl Response {
    /// Extract text payload, if any.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Extract binary payload, if any.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Binary(b) => Some(b),
            _ => None,
        }
    }

    /// Returns `true` if the response indicates success.
    pub fn is_ok(&self) -> bool {
        !matches!(self, Self::Error(_))
    }

    /// Extract error message, if any.
    pub fn as_error(&self) -> Option<&str> {
        match self {
            Self::Error(e) => Some(e),
            _ => None,
        }
    }
}

/// Serde helper for base64-encoding binary fields in JSON.
mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD.decode(s).map_err(serde::de::Error::custom)
    }
}
