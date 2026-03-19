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
