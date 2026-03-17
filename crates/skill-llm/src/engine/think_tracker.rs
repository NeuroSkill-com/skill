// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! `<think>…</think>` budget tracker for reasoning models.

/// Tracks the model's `<think>…</think>` block and enforces a token budget.
///
/// Feed every decoded piece via `feed()`.  When the budget is exhausted the
/// method returns `Some("\n</think>\n")` — that string should be:
///   1. Appended to the outgoing `pending` buffer (so the UI sees it), and
///   2. Tokenised and decoded into the KV cache (so the model continues from
///      a logically consistent state after the closing tag).
pub(super) struct ThinkTracker {
    budget:    Option<u32>,
    inside:    bool,
    closed:    bool,
    tag_buf:   String,   // accumulate chars to detect multi-token tags
    tok_count: u32,
}

impl ThinkTracker {
    pub fn new(budget: Option<u32>) -> Self {
        Self { budget, inside: false, closed: false, tag_buf: String::new(), tok_count: 0 }
    }

    /// Returns `Some(inject)` if the think block must be force-closed now.
    pub fn feed(&mut self, piece: &str) -> Option<String> {
        if self.closed { return None; }

        self.tag_buf.push_str(piece);
        // Keep tag_buf bounded — only need enough to detect the longest tag
        let cap = "</think>".len() + 4;
        if self.tag_buf.len() > cap * 2 {
            let drain = self.tag_buf.len() - cap;
            // Snap to a char boundary — raw byte arithmetic can land inside a
            // multi-byte codepoint (e.g. CJK) and cause a panic.
            let drain = (0..=drain).rev()
                .find(|&i| self.tag_buf.is_char_boundary(i))
                .unwrap_or(0);
            self.tag_buf.drain(..drain);
        }

        if !self.inside {
            // Detect <think> opening
            if self.tag_buf.contains("<think>") {
                self.inside = true;
                // Trim everything up to and including the opening tag
                if let Some(p) = self.tag_buf.find("<think>") {
                    self.tag_buf = self.tag_buf[p + 7..].to_string();
                }
            }
            return None;
        }

        // Inside the think block
        self.tok_count += 1;

        // Check for natural close
        if self.tag_buf.contains("</think>") {
            self.inside = false;
            self.closed = true;
            self.tag_buf.clear();
            return None;
        }

        // Enforce budget
        if let Some(budget) = self.budget {
            if self.tok_count >= budget {
                self.inside = false;
                self.closed = true;
                self.tag_buf.clear();
                return Some("\n</think>\n".to_string());
            }
        }
        None
    }
}
