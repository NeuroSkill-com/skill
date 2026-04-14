// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Persistent web cache for `web_search` and `web_fetch` tool results.
//!
//! Stores responses on disk in `<skill_dir>/web_cache/` as individual JSON
//! files keyed by a SHA-256 hash of the request parameters.  Each entry has a
//! TTL that depends on the content type and domain.
//!
//! The cache is designed for the LLM tool pipeline: it avoids redundant
//! network calls when the model re-fetches the same URL or re-runs the same
//! search query within a conversation.
//!
//! # Thread safety
//!
//! A single global [`WebCache`] instance is initialised once via
//! [`init_global`] and accessed via [`global`].  All methods are safe to call
//! from multiple threads (interior `RwLock`).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::types::WebCacheConfig;

// ── Global singleton ──────────────────────────────────────────────────────────

static GLOBAL: OnceLock<WebCache> = OnceLock::new();

/// Initialise the global web cache.  Safe to call multiple times — only the
/// first call has any effect.
pub fn init_global(cache_dir: PathBuf, config: WebCacheConfig) {
    let _ = GLOBAL.set(WebCache::new(cache_dir, config));
}

/// Update the configuration of the global cache (e.g. after settings change).
pub fn update_config(config: WebCacheConfig) {
    if let Some(cache) = GLOBAL.get() {
        if let Ok(mut guard) = cache.config.write() {
            *guard = config;
        }
    }
}

/// Return a reference to the global cache, if initialised.
pub fn global() -> Option<&'static WebCache> {
    GLOBAL.get()
}

// ── Cache entry ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CacheEntry {
    /// Cache key (hex-encoded SHA-256).
    key: String,
    /// What kind of request produced this entry.
    kind: CacheKind,
    /// The domain (for URL fetches) or search backend (for queries).
    domain: String,
    /// Unix timestamp (seconds) when the entry was stored.
    created_at: u64,
    /// TTL in seconds that was applied when storing.
    ttl_secs: u64,
    /// The cached JSON response (same shape as the tool result).
    data: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CacheKind {
    WebSearch,
    WebFetch,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        let now = unix_secs();
        now.saturating_sub(self.created_at) > self.ttl_secs
    }
}

// ── WebCache ──────────────────────────────────────────────────────────────────

pub struct WebCache {
    dir: PathBuf,
    config: RwLock<WebCacheConfig>,
}

impl WebCache {
    fn new(dir: PathBuf, config: WebCacheConfig) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Self {
            dir,
            config: RwLock::new(config),
        }
    }

    /// Look up a cached web_search result.
    /// Returns `Some(cached_response)` if a non-expired entry exists.
    pub fn get_search(&self, query: &str, backend: &str, render: bool) -> Option<Value> {
        if !self.is_enabled() {
            return None;
        }
        let key = cache_key_search(query, backend, render);
        self.get_entry(&key).map(|e| e.data)
    }

    /// Store a web_search result.
    pub fn put_search(&self, query: &str, backend: &str, render: bool, data: &Value) {
        if !self.is_enabled() {
            return;
        }
        let key = cache_key_search(query, backend, render);
        let ttl = self.config_read().search_ttl_secs;
        let entry = CacheEntry {
            key: key.clone(),
            kind: CacheKind::WebSearch,
            domain: backend.to_string(),
            created_at: unix_secs(),
            ttl_secs: ttl,
            data: data.clone(),
        };
        self.put_entry(&key, &entry);
    }

    /// Look up a cached web_fetch result.
    pub fn get_fetch(&self, url: &str, render: bool) -> Option<Value> {
        if !self.is_enabled() {
            return None;
        }
        let key = cache_key_fetch(url, render);
        let entry = self.get_entry(&key)?;

        // Check domain-specific TTL override.
        let domain = extract_domain(url);
        let cfg = self.config_read();
        if let Some(&override_ttl) = cfg.domain_ttl_overrides.get(&domain) {
            let now = unix_secs();
            if now.saturating_sub(entry.created_at) > override_ttl {
                // Expired per domain rule — remove stale file.
                let _ = std::fs::remove_file(self.entry_path(&key));
                return None;
            }
        }

        Some(entry.data)
    }

    /// Store a web_fetch result.
    pub fn put_fetch(&self, url: &str, render: bool, data: &Value) {
        if !self.is_enabled() {
            return;
        }
        let key = cache_key_fetch(url, render);
        let domain = extract_domain(url);
        let cfg = self.config_read();
        let ttl = cfg
            .domain_ttl_overrides
            .get(&domain)
            .copied()
            .unwrap_or(cfg.fetch_ttl_secs);
        drop(cfg);

        let entry = CacheEntry {
            key: key.clone(),
            kind: CacheKind::WebFetch,
            domain,
            created_at: unix_secs(),
            ttl_secs: ttl,
            data: data.clone(),
        };
        self.put_entry(&key, &entry);
    }

    /// Remove all expired entries from the cache directory.
    /// Called lazily (e.g. on startup or periodically).
    pub fn evict_expired(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(ce) = serde_json::from_str::<CacheEntry>(&data) {
                    if ce.is_expired() {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
    }

    /// Return cache statistics: total entries, expired entries, total bytes.
    pub fn stats(&self) -> CacheStats {
        let mut total = 0u64;
        let mut expired = 0u64;
        let mut bytes = 0u64;

        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                total += 1;
                bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
                if let Ok(data) = std::fs::read_to_string(&path) {
                    if let Ok(ce) = serde_json::from_str::<CacheEntry>(&data) {
                        if ce.is_expired() {
                            expired += 1;
                        }
                    }
                }
            }
        }

        CacheStats {
            total_entries: total,
            expired_entries: expired,
            total_bytes: bytes,
        }
    }

    /// List all non-expired cache entries as lightweight summaries.
    pub fn list_entries(&self) -> Vec<CacheEntrySummary> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(ce) = serde_json::from_str::<CacheEntry>(&data) {
                    if ce.is_expired() {
                        continue;
                    }
                    let label = match ce.kind {
                        CacheKind::WebSearch => ce
                            .data
                            .get("query")
                            .or_else(|| ce.data.get("compact"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.chars().take(80).collect::<String>())
                            .unwrap_or_else(|| ce.key[..12].to_string()),
                        CacheKind::WebFetch => ce
                            .data
                            .get("url")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&ce.domain)
                            .to_string(),
                    };
                    let bytes = data.len() as u64;
                    out.push(CacheEntrySummary {
                        key: ce.key,
                        kind: match ce.kind {
                            CacheKind::WebSearch => "search".into(),
                            CacheKind::WebFetch => "fetch".into(),
                        },
                        domain: ce.domain,
                        label,
                        created_at: ce.created_at,
                        ttl_secs: ce.ttl_secs,
                        bytes,
                    });
                }
            }
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        out
    }

    /// Remove all entries whose domain matches (case-insensitive).
    pub fn remove_by_domain(&self, domain: &str) -> u64 {
        let domain_lower = domain.to_lowercase();
        let mut removed = 0u64;
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(data) = std::fs::read_to_string(&path) {
                    if let Ok(ce) = serde_json::from_str::<CacheEntry>(&data) {
                        if ce.domain.to_lowercase() == domain_lower {
                            let _ = std::fs::remove_file(&path);
                            removed += 1;
                        }
                    }
                }
            }
        }
        removed
    }

    /// Remove a single entry by its cache key.
    pub fn remove_entry(&self, key: &str) -> bool {
        let path = self.entry_path(key);
        std::fs::remove_file(&path).is_ok()
    }

    /// Delete all cached entries.
    pub fn clear(&self) {
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }

    // ── Internal ──────────────────────────────────────────────────────────

    fn is_enabled(&self) -> bool {
        self.config_read().enabled
    }

    fn config_read(&self) -> std::sync::RwLockReadGuard<'_, WebCacheConfig> {
        self.config.read().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{}.json", key))
    }

    fn get_entry(&self, key: &str) -> Option<CacheEntry> {
        let path = self.entry_path(key);
        let data = std::fs::read_to_string(&path).ok()?;
        let entry: CacheEntry = serde_json::from_str(&data).ok()?;
        if entry.is_expired() {
            let _ = std::fs::remove_file(&path);
            return None;
        }
        Some(entry)
    }

    fn put_entry(&self, key: &str, entry: &CacheEntry) {
        let path = self.entry_path(key);
        if let Ok(json) = serde_json::to_string(entry) {
            let _ = std::fs::write(&path, json);
        }
    }
}

// ── Cache stats ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: u64,
    pub expired_entries: u64,
    pub total_bytes: u64,
}

/// Lightweight summary of a cache entry for UI display.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheEntrySummary {
    pub key: String,
    /// `"search"` or `"fetch"`.
    pub kind: String,
    pub domain: String,
    pub label: String,
    pub created_at: u64,
    pub ttl_secs: u64,
    pub bytes: u64,
}

// ── Key generation ────────────────────────────────────────────────────────────

fn cache_key_search(query: &str, backend: &str, render: bool) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"search\0");
    hasher.update(query.to_lowercase().as_bytes());
    hasher.update(b"\0");
    hasher.update(backend.as_bytes());
    hasher.update(b"\0");
    hasher.update(if render { b"r" } else { b"n" });
    hex_encode(&hasher.finalize())
}

fn cache_key_fetch(url: &str, render: bool) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"fetch\0");
    hasher.update(url.as_bytes());
    hasher.update(b"\0");
    hasher.update(if render { b"r" } else { b"n" });
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Extract the domain from a URL (e.g. `"https://example.com/path"` → `"example.com"`).
fn extract_domain(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .split(':')
        .next()
        .unwrap_or(url)
        .to_lowercase()
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    use std::sync::atomic::{AtomicU64, Ordering};
    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmp_cache(config: WebCacheConfig) -> WebCache {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("skill_web_cache_test_{}_{}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&dir);
        WebCache::new(dir, config)
    }

    fn default_config() -> WebCacheConfig {
        WebCacheConfig::default()
    }

    #[test]
    fn cache_disabled_returns_none() {
        let c = tmp_cache(WebCacheConfig {
            enabled: false,
            ..default_config()
        });
        c.put_fetch("https://example.com", false, &json!({"ok": true}));
        assert!(c.get_fetch("https://example.com", false).is_none());
        c.clear();
    }

    #[test]
    fn put_get_fetch() {
        let c = tmp_cache(default_config());
        let data = json!({"ok": true, "content": "hello"});
        c.put_fetch("https://example.com/page", false, &data);

        let cached = c.get_fetch("https://example.com/page", false);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap()["content"], "hello");

        // Different render mode = different key.
        assert!(c.get_fetch("https://example.com/page", true).is_none());
        c.clear();
    }

    #[test]
    fn put_get_search() {
        let c = tmp_cache(default_config());
        let data = json!({"ok": true, "results": [1, 2, 3]});
        c.put_search("rust async", "duckduckgo", false, &data);

        let cached = c.get_search("rust async", "duckduckgo", false);
        assert!(cached.is_some());

        // Case insensitive.
        let cached2 = c.get_search("Rust Async", "duckduckgo", false);
        assert!(cached2.is_some());

        // Different backend = miss.
        assert!(c.get_search("rust async", "brave", false).is_none());
        c.clear();
    }

    #[test]
    fn expired_entries_are_not_returned() {
        let c = tmp_cache(WebCacheConfig {
            fetch_ttl_secs: 0, // immediate expiry
            ..default_config()
        });
        c.put_fetch("https://example.com", false, &json!({"ok": true}));
        // TTL=0 means created_at == now, and 0 seconds have not passed yet.
        // So we need to check that after 1 second it expires. Instead, let's
        // test via the entry directly.
        let key = cache_key_fetch("https://example.com", false);
        let path = c.entry_path(&key);
        // Manually backdate created_at by 2 seconds.
        if let Ok(data) = std::fs::read_to_string(&path) {
            let mut entry: CacheEntry = serde_json::from_str(&data).unwrap();
            entry.created_at = unix_secs().saturating_sub(2);
            let _ = std::fs::write(&path, serde_json::to_string(&entry).unwrap());
        }
        assert!(c.get_fetch("https://example.com", false).is_none());
        c.clear();
    }

    #[test]
    fn domain_ttl_override() {
        let mut overrides = HashMap::new();
        overrides.insert("news.example.com".to_string(), 0u64); // immediate expiry
        let c = tmp_cache(WebCacheConfig {
            domain_ttl_overrides: overrides,
            fetch_ttl_secs: 3600,
            ..default_config()
        });
        c.put_fetch("https://news.example.com/article", false, &json!({"ok": true}));
        // Backdate.
        let key = cache_key_fetch("https://news.example.com/article", false);
        let path = c.entry_path(&key);
        if let Ok(data) = std::fs::read_to_string(&path) {
            let mut entry: CacheEntry = serde_json::from_str(&data).unwrap();
            entry.created_at = unix_secs().saturating_sub(2);
            let _ = std::fs::write(&path, serde_json::to_string(&entry).unwrap());
        }
        assert!(c.get_fetch("https://news.example.com/article", false).is_none());
        c.clear();
    }

    #[test]
    fn evict_expired_removes_stale() {
        let c = tmp_cache(WebCacheConfig {
            fetch_ttl_secs: 0,
            ..default_config()
        });
        c.put_fetch("https://a.com", false, &json!({"ok": true}));
        c.put_fetch("https://b.com", false, &json!({"ok": true}));
        // Backdate all entries.
        for entry in std::fs::read_dir(&c.dir).unwrap().flatten() {
            if let Ok(data) = std::fs::read_to_string(entry.path()) {
                if let Ok(mut ce) = serde_json::from_str::<CacheEntry>(&data) {
                    ce.created_at = unix_secs().saturating_sub(2);
                    let _ = std::fs::write(entry.path(), serde_json::to_string(&ce).unwrap());
                }
            }
        }
        let stats_before = c.stats();
        assert!(stats_before.total_entries >= 2);
        c.evict_expired();
        let stats_after = c.stats();
        assert_eq!(stats_after.total_entries, 0);
    }

    #[test]
    fn clear_removes_all() {
        let c = tmp_cache(default_config());
        c.put_fetch("https://a.com", false, &json!({}));
        c.put_fetch("https://b.com", false, &json!({}));
        c.put_search("q", "ddg", false, &json!({}));
        assert!(c.stats().total_entries >= 3);
        c.clear();
        assert_eq!(c.stats().total_entries, 0);
    }

    #[test]
    fn extract_domain_works() {
        assert_eq!(extract_domain("https://example.com/path"), "example.com");
        assert_eq!(extract_domain("http://sub.example.com:8080/x"), "sub.example.com");
        assert_eq!(extract_domain("https://Example.COM"), "example.com");
    }

    #[test]
    fn stats_counts_entries() {
        let c = tmp_cache(default_config());
        c.put_fetch("https://a.com", false, &json!({}));
        c.put_search("q", "ddg", false, &json!({}));
        let s = c.stats();
        assert_eq!(s.total_entries, 2);
        assert!(s.total_bytes > 0);
        assert_eq!(s.expired_entries, 0);
        c.clear();
    }

    // ── extract_domain ───────────────────────────────────────────────────

    #[test]
    fn extract_domain_https() {
        assert_eq!(extract_domain("https://example.com/path"), "example.com");
    }

    #[test]
    fn extract_domain_http_with_port() {
        assert_eq!(extract_domain("http://localhost:8080/api"), "localhost");
    }

    #[test]
    fn extract_domain_no_scheme() {
        assert_eq!(extract_domain("example.com/path"), "example.com");
    }

    #[test]
    fn extract_domain_bare() {
        assert_eq!(extract_domain("example.com"), "example.com");
    }

    #[test]
    fn extract_domain_uppercased() {
        assert_eq!(extract_domain("https://Example.COM/Page"), "example.com");
    }

    // ── hex_encode ───────────────────────────────────────────────────────

    #[test]
    fn hex_encode_empty() {
        assert_eq!(hex_encode(&[]), "");
    }

    #[test]
    fn hex_encode_known() {
        assert_eq!(hex_encode(&[0xff, 0x00, 0xab]), "ff00ab");
    }

    // ── cache_key determinism ────────────────────────────────────────────

    #[test]
    fn cache_key_search_deterministic() {
        let k1 = cache_key_search("test query", "default", false);
        let k2 = cache_key_search("test query", "default", false);
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn cache_key_search_varies_by_render() {
        let k1 = cache_key_search("test", "default", false);
        let k2 = cache_key_search("test", "default", true);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_search_varies_by_backend() {
        let k1 = cache_key_search("test", "google", false);
        let k2 = cache_key_search("test", "duckduckgo", false);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_search_case_insensitive_query() {
        let k1 = cache_key_search("Hello", "default", false);
        let k2 = cache_key_search("hello", "default", false);
        assert_eq!(k1, k2);
    }

    #[test]
    fn cache_key_fetch_deterministic() {
        let k1 = cache_key_fetch("https://example.com", false);
        let k2 = cache_key_fetch("https://example.com", false);
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 64);
    }

    #[test]
    fn cache_key_fetch_varies_by_url() {
        let k1 = cache_key_fetch("https://a.com", false);
        let k2 = cache_key_fetch("https://b.com", false);
        assert_ne!(k1, k2);
    }
}
