// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tool configuration types — shared between the skill-tools crate and the
//! main application / skill-llm.

use serde::{Deserialize, Serialize};

// ── Tool configuration ────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmToolConfig {
    /// Master switch — when `false`, *all* tools are disabled regardless of
    /// individual toggles.
    #[serde(default = "default_true")]
    pub enabled: bool,

    pub date:       bool,
    pub location:   bool,
    pub web_search: bool,
    pub web_fetch:  bool,

    /// Web search provider configuration.
    #[serde(default)]
    pub web_search_provider: WebSearchProvider,

    /// Allow the LLM to execute bash/shell commands.
    #[serde(default)]
    pub bash: bool,

    /// Allow the LLM to read file contents.
    #[serde(default)]
    pub read_file: bool,

    /// Allow the LLM to write/create files.
    #[serde(default)]
    pub write_file: bool,

    /// Allow the LLM to make surgical find-and-replace edits to files.
    #[serde(default)]
    pub edit_file: bool,

    /// Allow the LLM to query the Skill API (device status, sessions, labels,
    /// search, hooks, DND, calibrations, etc.) via the local WebSocket server.
    #[serde(default = "default_true")]
    pub skill_api: bool,

    /// The local WebSocket/HTTP port the Skill server is listening on.
    /// Set at runtime; not persisted.  Defaults to 0 (disabled).
    #[serde(skip)]
    pub skill_api_port: u16,

    /// Tool execution mode: "parallel" or "sequential".
    /// Parallel: prepare sequentially, execute concurrently.
    /// Sequential: prepare and execute one at a time.
    #[serde(default = "default_tool_execution_mode")]
    pub execution_mode: ToolExecutionMode,

    /// Maximum number of tool-calling rounds per chat turn.
    #[serde(default = "default_max_tool_rounds")]
    pub max_rounds: usize,

    /// Maximum number of tool calls executed per round.
    #[serde(default = "default_max_tool_calls_per_round")]
    pub max_calls_per_round: usize,

    /// Thinking budget override for tool-calling rounds.
    ///
    /// Controls how many tokens the model may spend inside `<think>…</think>`
    /// blocks during tool-calling inference rounds.
    ///
    /// - `None` = use the chat-level thinking budget (no override).
    /// - `Some(0)` = skip thinking entirely during tool rounds.
    /// - `Some(n)` = cap thinking to `n` tokens during tool rounds.
    ///
    /// Lower values make the model respond faster after tool results.
    #[serde(default)]
    pub thinking_budget: Option<u32>,

    /// Context compression settings for tool results.
    #[serde(default)]
    pub context_compression: ToolContextCompression,

    /// Seconds between automatic community-skills refresh from GitHub.
    /// `0` = disabled.  Default: 86 400 (24 hours).
    #[serde(default = "default_skills_refresh_interval")]
    pub skills_refresh_interval_secs: u64,

    /// Skill names that are explicitly disabled (will not be injected into the
    /// LLM system prompt).  Empty = all discovered skills are available.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_skills: Vec<String>,
}

/// Web search provider configuration.
///
/// Search order: the configured provider is tried first, with DuckDuckGo HTML
/// scraping as a final fallback.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WebSearchProvider {
    /// Which search backend to use: `"duckduckgo"`, `"brave"`, or `"searxng"`.
    #[serde(default = "default_search_backend")]
    pub backend: String,

    /// Brave Search API key (free tier: 2 000 queries/month).
    /// Get one at <https://brave.com/search/api/>.
    #[serde(default)]
    pub brave_api_key: String,

    /// Self-hosted SearXNG instance base URL (e.g. `"https://search.example.com"`).
    #[serde(default)]
    pub searxng_url: String,
}

fn default_search_backend() -> String { "duckduckgo".into() }

impl Default for WebSearchProvider {
    fn default() -> Self {
        Self {
            backend:       default_search_backend(),
            brave_api_key: String::new(),
            searxng_url:   String::new(),
        }
    }
}

/// How tool calls from a single assistant message are executed.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolExecutionMode {
    /// Execute tool calls one by one in order.
    Sequential,
    /// Prepare sequentially, then execute allowed tools concurrently.
    Parallel,
}

/// How aggressively tool results are compressed to save context window space.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompressionLevel {
    /// No compression — tool results are kept as-is.
    Off,
    /// Moderate: cap web search results, truncate long URLs, compress old
    /// results after a few rounds.  Good balance for 4 K–8 K context windows.
    Normal,
    /// Aggressive: fewer search results, tighter character limits, old tool
    /// results summarised to a single line.  Best for small (≤ 4 K) contexts.
    Aggressive,
}

/// Settings that control how tool results are compressed before they are
/// injected back into the conversation context.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolContextCompression {
    /// Compression level.
    #[serde(default = "default_compression_level")]
    pub level: CompressionLevel,

    /// Maximum number of web search results returned (0 = use default per level).
    #[serde(default)]
    pub max_search_results: usize,

    /// Maximum characters kept per tool result (0 = use default per level).
    #[serde(default)]
    pub max_result_chars: usize,
}

fn default_compression_level() -> CompressionLevel { CompressionLevel::Normal }

impl Default for ToolContextCompression {
    fn default() -> Self {
        Self {
            level: default_compression_level(),
            max_search_results: 0,
            max_result_chars: 0,
        }
    }
}

impl ToolContextCompression {
    /// Effective max search results based on level and override.
    pub fn effective_max_search_results(&self) -> usize {
        if self.max_search_results > 0 { return self.max_search_results; }
        match self.level {
            CompressionLevel::Off        => 10,
            CompressionLevel::Normal     => 5,
            CompressionLevel::Aggressive => 3,
        }
    }

    /// Effective max chars per tool result based on level and override.
    pub fn effective_max_result_chars(&self) -> usize {
        if self.max_result_chars > 0 { return self.max_result_chars; }
        match self.level {
            CompressionLevel::Off        => 16_000,
            CompressionLevel::Normal     => 2_000,
            CompressionLevel::Aggressive => 1_000,
        }
    }

    /// Effective max chars for web search results (tighter than general).
    pub fn effective_max_search_result_chars(&self) -> usize {
        match self.level {
            CompressionLevel::Off        => 16_000,
            CompressionLevel::Normal     => 1_500,
            CompressionLevel::Aggressive => 800,
        }
    }

    /// Whether to truncate long URLs in search results.
    pub fn should_truncate_urls(&self) -> bool {
        self.level != CompressionLevel::Off
    }

    /// Whether to aggressively compress old (non-recent) tool results.
    pub fn should_compress_old_results(&self) -> bool {
        self.level != CompressionLevel::Off
    }
}

fn default_true()                      -> bool { true }
fn default_tool_execution_mode()       -> ToolExecutionMode { ToolExecutionMode::Parallel }
fn default_max_tool_rounds()           -> usize { 3 }
fn default_max_tool_calls_per_round()  -> usize { 4 }
fn default_skills_refresh_interval()   -> u64  { 86_400 }

impl Default for LlmToolConfig {
    fn default() -> Self {
        Self {
            enabled:            true,
            date:               true,
            location:           true,
            web_search:         true,
            web_fetch:          true,
            web_search_provider: WebSearchProvider::default(),
            bash:               false,
            read_file:          false,
            write_file:         false,
            edit_file:          false,
            skill_api:          true,
            skill_api_port:     0,
            execution_mode:     default_tool_execution_mode(),
            max_rounds:         10,
            max_calls_per_round: default_max_tool_calls_per_round(),
            thinking_budget:    None,
            context_compression: ToolContextCompression::default(),
            skills_refresh_interval_secs: default_skills_refresh_interval(),
            disabled_skills: Vec::new(),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── LlmToolConfig defaults ────────────────────────────────────────────

    #[test]
    fn default_config_has_tools_enabled() {
        let cfg = LlmToolConfig::default();
        assert!(cfg.enabled);
        assert!(cfg.date);
        assert!(cfg.web_search);
        assert!(cfg.skill_api);
    }

    #[test]
    fn default_config_has_dangerous_tools_disabled() {
        let cfg = LlmToolConfig::default();
        assert!(!cfg.bash);
        assert!(!cfg.read_file);
        assert!(!cfg.write_file);
        assert!(!cfg.edit_file);
    }

    #[test]
    fn default_execution_mode_is_parallel() {
        assert_eq!(LlmToolConfig::default().execution_mode, ToolExecutionMode::Parallel);
    }

    #[test]
    fn default_max_rounds_is_positive() {
        assert!(LlmToolConfig::default().max_rounds > 0);
    }

    #[test]
    fn default_skills_refresh_interval_is_24h() {
        assert_eq!(LlmToolConfig::default().skills_refresh_interval_secs, 86_400);
    }

    // ── JSON round-trip ───────────────────────────────────────────────────

    #[test]
    fn config_round_trips_through_json() {
        let cfg = LlmToolConfig {
            bash: true,
            read_file: true,
            max_rounds: 5,
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: LlmToolConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.bash);
        assert!(parsed.read_file);
        assert_eq!(parsed.max_rounds, 5);
    }

    #[test]
    fn config_deserialises_from_empty_json() {
        let cfg: LlmToolConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.enabled);
        assert!(!cfg.bash);
        assert_eq!(cfg.execution_mode, ToolExecutionMode::Parallel);
    }

    #[test]
    fn skill_api_port_is_not_serialised() {
        let mut cfg = LlmToolConfig::default();
        cfg.skill_api_port = 9999;
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("skill_api_port"), "skip field should not appear in JSON");
        let parsed: LlmToolConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.skill_api_port, 0);
    }

    // ── WebSearchProvider ─────────────────────────────────────────────────

    #[test]
    fn default_search_provider_is_duckduckgo() {
        let p = WebSearchProvider::default();
        assert_eq!(p.backend, "duckduckgo");
        assert!(p.brave_api_key.is_empty());
        assert!(p.searxng_url.is_empty());
    }

    // ── ToolExecutionMode ─────────────────────────────────────────────────

    #[test]
    fn execution_mode_serialises_lowercase() {
        let json = serde_json::to_string(&ToolExecutionMode::Sequential).unwrap();
        assert_eq!(json, "\"sequential\"");
        let json = serde_json::to_string(&ToolExecutionMode::Parallel).unwrap();
        assert_eq!(json, "\"parallel\"");
    }

    #[test]
    fn execution_mode_deserialises_lowercase() {
        let p: ToolExecutionMode = serde_json::from_str("\"parallel\"").unwrap();
        assert_eq!(p, ToolExecutionMode::Parallel);
        let s: ToolExecutionMode = serde_json::from_str("\"sequential\"").unwrap();
        assert_eq!(s, ToolExecutionMode::Sequential);
    }

    // ── CompressionLevel ──────────────────────────────────────────────────

    #[test]
    fn compression_level_serialises_lowercase() {
        assert_eq!(serde_json::to_string(&CompressionLevel::Off).unwrap(), "\"off\"");
        assert_eq!(serde_json::to_string(&CompressionLevel::Normal).unwrap(), "\"normal\"");
        assert_eq!(serde_json::to_string(&CompressionLevel::Aggressive).unwrap(), "\"aggressive\"");
    }

    // ── ToolContextCompression ────────────────────────────────────────────

    #[test]
    fn default_compression_is_normal() {
        let c = ToolContextCompression::default();
        assert_eq!(c.level, CompressionLevel::Normal);
        assert_eq!(c.max_search_results, 0);
        assert_eq!(c.max_result_chars, 0);
    }

    #[test]
    fn compression_off_has_highest_limits() {
        let off = ToolContextCompression { level: CompressionLevel::Off, ..Default::default() };
        assert_eq!(off.effective_max_search_results(), 10);
        assert_eq!(off.effective_max_result_chars(), 16_000);
        assert!(!off.should_truncate_urls());
        assert!(!off.should_compress_old_results());
    }

    #[test]
    fn compression_aggressive_has_lowest_limits() {
        let agg = ToolContextCompression { level: CompressionLevel::Aggressive, ..Default::default() };
        assert!(agg.effective_max_search_results() <= 3);
        assert!(agg.effective_max_result_chars() <= 1_000);
        assert!(agg.should_truncate_urls());
        assert!(agg.should_compress_old_results());
    }

    #[test]
    fn compression_custom_overrides_level_defaults() {
        let c = ToolContextCompression {
            level: CompressionLevel::Off,
            max_search_results: 2,
            max_result_chars: 500,
        };
        assert_eq!(c.effective_max_search_results(), 2);
        assert_eq!(c.effective_max_result_chars(), 500);
    }

    // ── Disabled skills serialisation ─────────────────────────────────────

    #[test]
    fn empty_disabled_skills_is_not_serialised() {
        let cfg = LlmToolConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("disabled_skills"));
    }

    #[test]
    fn nonempty_disabled_skills_is_serialised() {
        let cfg = LlmToolConfig {
            disabled_skills: vec!["some-skill".into()],
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("disabled_skills"));
        assert!(json.contains("some-skill"));
    }
}
