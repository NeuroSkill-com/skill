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

fn default_true()                      -> bool { true }
fn default_tool_execution_mode()       -> ToolExecutionMode { ToolExecutionMode::Parallel }
fn default_max_tool_rounds()           -> usize { 3 }
fn default_max_tool_calls_per_round()  -> usize { 4 }

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
        }
    }
}
