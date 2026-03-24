// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Text truncation helpers for tool output.

/// Truncate a string to at most `max_chars` characters.
pub fn truncate_text(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

pub(crate) struct TruncatedOutput {
    pub text: String,
    pub was_truncated: bool,
    #[cfg_attr(not(test), allow(dead_code))]
    pub total_lines: usize,
    #[allow(dead_code)]
    pub total_bytes: usize,
    pub output_lines: usize,
}

/// Truncate from the tail (keep last N lines / max bytes).
/// Suitable for bash output where the end (errors/results) matters most.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn truncate_tool_output(content: &str, max_lines: usize, max_bytes: usize) -> TruncatedOutput {
    let total_bytes = content.len();
    let lines: Vec<&str> = content.split('\n').collect();
    let total_lines = lines.len();

    if total_lines <= max_lines && total_bytes <= max_bytes {
        return TruncatedOutput {
            text: content.to_string(),
            was_truncated: false,
            total_lines,
            total_bytes,
            output_lines: total_lines,
        };
    }

    let mut output: Vec<&str> = Vec::new();
    let mut byte_count = 0usize;

    for &line in lines.iter().rev() {
        let lb = line.len() + if output.is_empty() { 0 } else { 1 };
        if byte_count + lb > max_bytes || output.len() >= max_lines {
            break;
        }
        output.push(line);
        byte_count += lb;
    }

    output.reverse();
    let output_lines = output.len();
    TruncatedOutput {
        text: output.join("\n"),
        was_truncated: true,
        total_lines,
        total_bytes,
        output_lines,
    }
}

/// Truncate from the head (keep first N lines / max bytes).
/// Suitable for file reads where you want to see the beginning.
pub(crate) fn truncate_tool_output_head(content: &str, max_lines: usize, max_bytes: usize) -> TruncatedOutput {
    let total_bytes = content.len();
    let lines: Vec<&str> = content.split('\n').collect();
    let total_lines = lines.len();

    if total_lines <= max_lines && total_bytes <= max_bytes {
        return TruncatedOutput {
            text: content.to_string(),
            was_truncated: false,
            total_lines,
            total_bytes,
            output_lines: total_lines,
        };
    }

    let mut output: Vec<&str> = Vec::new();
    let mut byte_count = 0usize;

    for &line in &lines {
        let lb = line.len() + if output.is_empty() { 0 } else { 1 };
        if byte_count + lb > max_bytes || output.len() >= max_lines {
            break;
        }
        output.push(line);
        byte_count += lb;
    }

    let output_lines = output.len();
    TruncatedOutput {
        text: output.join("\n"),
        was_truncated: true,
        total_lines,
        total_bytes,
        output_lines,
    }
}
