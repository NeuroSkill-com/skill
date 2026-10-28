// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Balanced JSON object / array range finders.
//!
//! These scan raw text for top-level `{…}` and `[…]` ranges while correctly
//! handling string escapes, so that JSON blobs embedded in prose can be
//! extracted without a full parser.

/// Find all top-level balanced `{…}` ranges in `content`.
///
/// Returns a vec of `(start, end)` byte offsets (end is exclusive).
pub fn find_balanced_json_objects(content: &str) -> Vec<(usize, usize)> {
    find_balanced(content, b'{', b'}')
}

/// Find all top-level balanced `[…]` ranges in `content`.
///
/// Returns a vec of `(start, end)` byte offsets (end is exclusive).
pub fn find_balanced_json_arrays(content: &str) -> Vec<(usize, usize)> {
    find_balanced(content, b'[', b']')
}

/// Close a JSON value that was cut short of its final brackets.
///
/// Small models often drop the last `}` of a nested tool call:
/// `{"name":"skill","arguments":{"command":"status","args":{"k":5}}` is three
/// opens and two closes. Returns the text with the missing closers appended,
/// or `None` unless it looks like exactly that: it must end on a closer (the
/// model closed some levels, just not all), so a value cut off mid-way —
/// `{"name":"date","arguments":{` — is not "repaired" into a call. Also `None`
/// when balanced, inside a string, or with a mismatched closer. Only use it
/// where something else bounds the value (e.g. a `[/TOOL_CALL]` tag), so the
/// repair can't swallow prose.
pub fn close_unbalanced_json(content: &str) -> Option<String> {
    if !content.trim_end().ends_with(['}', ']']) {
        return None;
    }
    let mut stack = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for &b in content.trim_end().as_bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => stack.push(b'}'),
            b'[' => stack.push(b']'),
            b'}' | b']' if stack.pop() != Some(b) => return None,
            _ => {}
        }
    }
    if in_string || stack.is_empty() {
        return None;
    }
    let mut out = content.trim_end().to_string();
    out.extend(stack.iter().rev().map(|&c| c as char));
    Some(out)
}

fn find_balanced(content: &str, open: u8, close: u8) -> Vec<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut out = Vec::new();

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut start = None::<usize>;

    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match b {
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match b {
            b'"' => in_string = true,
            b if b == open => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b if b == close => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        out.push((s, i + 1));
                    }
                }
            }
            _ => {}
        }
    }

    out
}
